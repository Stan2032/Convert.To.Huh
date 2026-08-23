//! Running external engines without trusting them.
//!
//! ADR-0003: engines are hostile and unreliable. They parse bytes written by
//! strangers, several have escaped their own sandboxes in the wild, and one of
//! them (ImageMagick) delegates to another (Ghostscript) by default, which is
//! how a number of the worst historical exploits were reached.
//!
//! Everything here follows from that. The four rules, in order of how often
//! they are broken elsewhere:
//!
//! 1. **Argument vectors, never shell strings.** No string is ever handed to a
//!    shell, so a filename containing `;` or `$(...)` is a filename.
//! 2. **Every run is bounded.** A wall-clock timeout is enforced by killing the
//!    child, because engines hang on malformed input rather than erroring.
//! 3. **Failures are classified, not just detected.** "Non-zero exit" tells an
//!    operator nothing; `input_malformed` versus `budget_exceeded` versus
//!    `engine_crash` decides whether to retry, refuse, or page someone.
//! 4. **Work happens in a scratch directory that is removed afterwards**, so a
//!    crashed engine cannot leave a half-written file to be mistaken for output.
//!
//! What this module does NOT provide, and must not be mistaken for: kernel-level
//! isolation. There is no namespace, seccomp filter or gVisor here. The registry
//! records an `isolation` tier per engine, and honouring it is the job of the
//! deployment, not of this file. Dangerzone runs its converters in a gVisor
//! sandbox inside a container; that is the standard to meet, and this is not it.

use crate::engines::ConvertError;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A bounded, classified invocation of an external program.
pub struct External {
    program: String,
    args: Vec<String>,
    timeout: Duration,
}

/// A scratch directory that removes itself.
///
/// Engines are given paths inside this and nowhere else. Dropping it removes
/// the tree, so a crash leaves no partial artifact that a later step could pick
/// up and treat as a successful conversion.
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub fn new(tag: &str) -> Result<Scratch, ConvertError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);

        // `temp_dir()` is normally /tmp: world-writable and shared with every
        // other local user. The old name was pid + counter and the directory was
        // made with `create_dir_all`, which SUCCEEDS on a path that already
        // exists — including one another user pre-created as a symlink. Engine
        // input and output then live in a directory someone else controls.
        //
        // Two independent defences, because either alone is thin:
        //   1. `create_dir` (not `_all`) fails when the path exists, so a
        //      pre-created path is an error rather than a silent adoption.
        //   2. An unpredictable suffix, so it cannot be pre-created to begin
        //      with — and, on a miss, retry rather than fail.
        for _ in 0..8 {
            let path = std::env::temp_dir().join(format!(
                "huh-{tag}-{}-{}-{:x}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed),
                scratch_entropy()
            ));
            match claim(&path)? {
                Some(s) => return Ok(s),
                None => continue, // occupied: try another name, never adopt it
            }
        }
        Err(ConvertError::Internal(
            "scratch dir: could not create an unused path in the temp directory".into(),
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Take exclusive ownership of `path` as a scratch directory.
///
/// `Ok(Some(_))` — created by us, owner-only.
/// `Ok(None)`    — something is already there. **Never adopted**: that path may
///                 belong to another local user, as a directory or a symlink
///                 pointing somewhere they can read.
/// `Err(_)`      — the filesystem said something else went wrong.
///
/// Split out from `Scratch::new` so the refusal can be tested directly. The
/// name carries unpredictable bits precisely so an attacker cannot arrange the
/// collision, which also means a test cannot arrange one through `new`.
fn claim(path: &Path) -> Result<Option<Scratch>, ConvertError> {
    // Owner-only ATOMICALLY. `create_dir` applies the umask, so under a typical
    // 022 the directory exists as 0755 until a later chmod lands — a window in
    // a shared /tmp where another user can open it. `DirBuilder::mode` closes
    // the window rather than narrowing it.
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut b = std::fs::DirBuilder::new();
        b.mode(0o700);
        b
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();

    match builder.create(path) {
        Ok(()) => {
            // Still called, for platforms whose DirBuilder has no mode.
            restrict_to_owner(path);
            Ok(Some(Scratch { path: path.to_path_buf() }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(e) => Err(ConvertError::Internal(format!("scratch dir: {e}"))),
    }
}

/// Unpredictable bits for a scratch directory name.
///
/// `/dev/urandom` where it exists, so the value does not come from anything an
/// observer can also read off the clock. The time-based fallback is weaker and
/// is only a fallback — exclusive creation, not this, is what makes the
/// directory safe.
fn scratch_entropy() -> u64 {
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let mut b = [0u8; 8];
            if f.read_exact(&mut b).is_ok() {
                return u64::from_ne_bytes(b);
            }
        }
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Make a scratch directory readable only by its owner, where the platform
/// has a notion of that. Best-effort: the exclusive create above is the
/// load-bearing part.
fn restrict_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[derive(Debug)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub elapsed: Duration,
}

impl External {
    /// Build an invocation. `program` is looked up on PATH; it is never
    /// interpreted by a shell.
    pub fn new(program: &str) -> External {
        External {
            program: program.to_string(),
            args: Vec::new(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn arg(mut self, a: impl AsRef<str>) -> External {
        self.args.push(a.as_ref().to_string());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> External
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for a in it {
            self.args.push(a.as_ref().to_string());
        }
        self
    }

    /// A path argument. Separate from `arg` so the call site reads as data
    /// rather than as text, and so this stays the obvious place to add
    /// path-confinement checks later.
    pub fn path_arg(mut self, p: &Path) -> External {
        self.args.push(p.to_string_lossy().to_string());
        self
    }

    pub fn timeout(mut self, d: Duration) -> External {
        self.timeout = d;
        self
    }

    /// The exact command, for the receipt and for the registry's `invocation`
    /// field (ADR-0013). Quoted for human legibility only — it is never parsed
    /// or executed as text.
    pub fn describe(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            if a.contains(' ') {
                s.push('"');
                s.push_str(a);
                s.push('"');
            } else {
                s.push_str(a);
            }
        }
        s
    }

    /// Run it, enforcing the timeout and classifying the outcome.
    pub fn run(&self) -> Result<Output, ConvertError> {
        let started = Instant::now();

        let mut child = Command::new(&self.program)
            .args(&self.args)
            // Nothing is read from the parent's stdin. An engine that blocks
            // waiting for input would otherwise hang until the timeout.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ConvertError::InputUnsupported(format!(
                    "engine `{}` is not installed",
                    self.program
                )),
                _ => ConvertError::Internal(format!("spawn {}: {e}", self.program)),
            })?;

        // Drain both pipes on their own threads, STARTING NOW rather than after
        // the child exits. A pipe holds a bounded amount — 64 KiB on Linux — and
        // a child whose pipe is full blocks on write. It then never exits, so
        // the poll loop below runs to the timeout and kills it.
        //
        // That failure mode is especially nasty here because it is misreported:
        // the caller sees `BudgetExceeded` and blames a hung engine. Every
        // engine this module runs writes more than 64 KiB. Found by review, then
        // reproduced by the two tests at the bottom of this file.
        let mut child_out = child.stdout.take();
        let mut child_err = child.stderr.take();
        let drain_out = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(s) = child_out.as_mut() {
                let _ = std::io::Read::read_to_end(s, &mut buf);
            }
            buf
        });
        let drain_err = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(s) = child_err.as_mut() {
                let _ = std::io::Read::read_to_end(s, &mut buf);
            }
            buf
        });

        // Poll rather than block, so the timeout can be enforced. std has no
        // wait-with-timeout; sleeping briefly between checks costs a few ms of
        // latency and avoids a dependency.
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    if started.elapsed() > self.timeout {
                        // Killing the child closes its ends of both pipes, so
                        // the drain threads reach EOF and finish on their own.
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = drain_out.join();
                        let _ = drain_err.join();
                        return Err(ConvertError::BudgetExceeded(format!(
                            "`{}` exceeded {}s and was killed",
                            self.program,
                            self.timeout.as_secs()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(ConvertError::Internal(format!("wait: {e}"))),
            }
        };

        // The child has exited, so both pipes are at EOF and these joins return
        // promptly with everything that was written.
        let stdout = drain_out
            .join()
            .map_err(|_| ConvertError::Internal("stdout reader panicked".into()))?;
        let stderr_raw = drain_err
            .join()
            .map_err(|_| ConvertError::Internal("stderr reader panicked".into()))?;
        let stderr = String::from_utf8_lossy(&stderr_raw).to_string();
        let elapsed = started.elapsed();

        if status.success() {
            return Ok(Output { stdout, stderr, elapsed });
        }

        Err(classify(&self.program, status.code(), &stderr))
    }
}

/// Map an engine's failure onto the taxonomy in `docs/specs/engine-manifest.md`.
///
/// Deliberately conservative: anything not clearly attributable to the input is
/// an `EngineCrash`, because misreporting a crash as bad input hides real bugs
/// and makes a retry look reasonable when it is not.
fn classify(program: &str, code: Option<i32>, stderr: &str) -> ConvertError {
    let s = stderr.to_ascii_lowercase();

    // Signals: no exit code. On Unix a killed child usually means it was
    // stopped by the OS — OOM being by far the most common cause here.
    if code.is_none() {
        return ConvertError::EngineCrash(format!(
            "`{program}` terminated by signal (frequently the OOM killer); stderr: {}",
            first_line(stderr)
        ));
    }

    let malformed = [
        "syntax error", "invalid", "malformed", "corrupt", "not a ",
        "unexpected end", "premature", "damaged", "unsupported file",
        "cannot be read", "bad file",
    ];
    if malformed.iter().any(|m| s.contains(m)) {
        return ConvertError::InputMalformed(format!(
            "`{program}` rejected the input: {}",
            first_line(stderr)
        ));
    }

    if s.contains("out of memory") || s.contains("cannot allocate") || s.contains("memory exhausted")
    {
        return ConvertError::BudgetExceeded(format!(
            "`{program}` ran out of memory: {}",
            first_line(stderr)
        ));
    }

    if s.contains("no such file") || s.contains("permission denied") {
        return ConvertError::Internal(format!(
            "`{program}` could not access its working files: {}",
            first_line(stderr)
        ));
    }

    ConvertError::EngineCrash(format!(
        "`{program}` exited {}: {}",
        code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
        first_line(stderr)
    ))
}

fn first_line(s: &str) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("(no output)");
    let line = line.trim();
    // Truncate by CHARACTER, not by byte. `line.len()` is a byte count, and
    // slicing at a byte index inside a multi-byte character panics — in the
    // error-reporting path, which is the worst place to panic. Engine stderr
    // carries non-ASCII in filenames and localized messages, and the U+FFFD
    // that `from_utf8_lossy` emits for invalid bytes is itself three bytes.
    match line.char_indices().nth(200) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_captures_output() {
        let out = External::new("echo").arg("hello").run().expect("echo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
    }

    #[test]
    fn output_larger_than_the_pipe_buffer_does_not_hang() {
        // An OS pipe holds a fixed amount — 64 KiB on Linux. If nothing drains
        // it while the child runs, the child blocks on write, never exits, and
        // the timeout fires: a real engine bug reported as a fake timeout.
        //
        // Every engine here writes more than this. `echo hello` cannot catch
        // it, which is why the bug survived until CodeRabbit flagged it.
        const N: usize = 400_000;
        let out = External::new("sh")
            .arg("-c")
            .arg(format!("yes ABCDEFGH | head -c {N}"))
            .timeout(Duration::from_secs(20))
            .run()
            .expect("large output must not be reported as a timeout");
        assert_eq!(out.stdout.len(), N, "output was truncated");
    }

    #[test]
    fn scratch_refuses_a_pre_existing_path_and_is_owner_only() {
        let a = Scratch::new("racetest").expect("scratch");
        let b = Scratch::new("racetest").expect("scratch");
        assert_ne!(a.path(), b.path(), "two scratch dirs collided");

        // Nobody else may read what an engine writes here.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(a.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "scratch dir is group/world accessible");
        }

        // The actual regression: an occupied path must be REFUSED, not adopted.
        // Tested through `claim`, because the unpredictable suffix that makes
        // the collision hard for an attacker to arrange also makes it
        // impossible for a test to arrange through `Scratch::new`.
        let occupied = a.path().join("someone-elses");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("their-file"), b"do not touch").unwrap();

        assert!(
            claim(&occupied).expect("an occupied path is not an error").is_none(),
            "Scratch adopted a directory it did not create"
        );
        // And refusing must not have disturbed what was there.
        assert!(occupied.join("their-file").exists(), "refusal deleted someone else's data");

        // A free path is claimed, owner-only.
        let fresh = a.path().join("fresh");
        let got = claim(&fresh).expect("claimable").expect("should have been claimed");
        assert_eq!(got.path(), fresh);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o077,
                0,
                "claimed directory is group/world accessible"
            );
        }
    }
    #[test]
    fn long_non_ascii_stderr_truncates_without_panicking() {
        // Byte 200 lands inside the 3-byte '€', so a byte-indexed slice panics.
        let line = format!("{}{}", "a".repeat(199), "€".repeat(20));
        let got = first_line(&line);
        assert!(got.ends_with('…'), "expected truncation, got {got:?}");
        // 200 characters kept, plus the ellipsis.
        assert_eq!(got.chars().count(), 201);
    }

    #[test]
    fn large_stderr_alone_does_not_hang() {
        // A verbose engine fills stderr without writing any stdout at all.
        const N: usize = 400_000;
        let out = External::new("sh")
            .arg("-c")
            .arg(format!("yes ABCDEFGH | head -c {N} >&2"))
            .timeout(Duration::from_secs(20))
            .run()
            .expect("large stderr must not be reported as a timeout");
        assert_eq!(out.stderr.len(), N, "stderr was truncated");
    }

    #[test]
    fn arguments_are_never_shell_interpreted() {
        // If this reached a shell it would substitute a command. It must come
        // back verbatim — this is the single most important property here.
        let payload = "$(touch /tmp/huh-should-not-exist); rm -rf /";
        let out = External::new("echo").arg(payload).run().expect("echo");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), payload);
        assert!(!std::path::Path::new("/tmp/huh-should-not-exist").exists());
    }

    #[test]
    fn timeout_kills_a_hanging_engine() {
        let started = Instant::now();
        let err = External::new("sleep")
            .arg("30")
            .timeout(Duration::from_millis(300))
            .run()
            .expect_err("should time out");
        assert!(matches!(err, ConvertError::BudgetExceeded(_)), "{err}");
        // Proves the child was actually killed rather than waited out.
        assert!(started.elapsed() < Duration::from_secs(5), "did not kill promptly");
    }

    #[test]
    fn missing_engine_is_unsupported_not_crash() {
        // An engine that is not installed is a capability gap, not a failure of
        // the conversion — the distinction decides whether to try another route.
        let err = External::new("huh-definitely-not-a-real-program")
            .run()
            .expect_err("should fail");
        assert!(matches!(err, ConvertError::InputUnsupported(_)), "{err}");
    }

    #[test]
    fn nonzero_exit_is_classified() {
        let err = External::new("sh").args(["-c", "exit 3"]).run().expect_err("should fail");
        assert!(matches!(err, ConvertError::EngineCrash(_)), "{err}");
    }

    #[test]
    fn scratch_removes_itself() {
        let p = {
            let s = Scratch::new("test").unwrap();
            let f = s.join("work.bin");
            std::fs::write(&f, b"x").unwrap();
            assert!(f.exists());
            s.path().to_path_buf()
        };
        assert!(!p.exists(), "scratch survived drop");
    }

    #[test]
    fn describe_reports_the_exact_invocation() {
        let e = External::new("vips").arg("webpsave").arg("in.png").arg("out.webp");
        assert_eq!(e.describe(), "vips webpsave in.png out.webp");
    }
}
