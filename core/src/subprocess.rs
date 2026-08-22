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
        // Process id plus a monotonic counter: unique without pulling in a
        // random-number dependency, and predictable enough to debug.
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "huh-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|e| ConvertError::Internal(format!("scratch dir: {e}")))?;
        Ok(Scratch { path })
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

        // Poll rather than block, so the timeout can be enforced. std has no
        // wait-with-timeout; sleeping briefly between checks costs a few ms of
        // latency and avoids a dependency.
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    if started.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
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

        let out = child
            .wait_with_output()
            .map_err(|e| ConvertError::Internal(format!("collect output: {e}")))?;
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let elapsed = started.elapsed();

        if status.success() {
            return Ok(Output { stdout: out.stdout, stderr, elapsed });
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
    if line.len() > 200 {
        format!("{}…", &line[..200])
    } else {
        line.to_string()
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
