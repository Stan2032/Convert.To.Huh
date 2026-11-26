//! `huh` — the CLI for Convert.To.Huh.
//!
//! Produces and checks the conversion-loss dataset (ADR-0011). Run with no
//! arguments for usage.

use huh_core::{clean, detect, execute, export, planner, registry::Registry};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn repo_root() -> PathBuf {
    // Walk up until we find registry/VERSION, so the CLI works from anywhere.
    let mut d = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if d.join("registry").join("VERSION").is_file() {
            return d;
        }
        if !d.pop() {
            return PathBuf::from(".");
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "huh — a planner over a typed conversion graph\n\n\
         USAGE:\n  \
           huh detect <file>\n  \
           huh plan <from> <to> [-k N] [--allow-unverified] [--profile lgpl|gpl] [--prefer prop=policy]\n  \
           huh suggest <file> [--allow-unverified]   what can this become, and what does each cost?\n  \
           huh convert <in> <out> [--quality 1-100] [--allow-unverified] [--receipt out.json]\n  \
           huh clean <in> <out> [--dpi N] [--report r.json]   detect, rebuild, report\n  \
           huh graph\n  \
           huh export [--out FILE]      emit the registry as a public dataset\n\n\
         Formats are registry ids, e.g. image/png, image/jpeg."
    );
    ExitCode::from(2)
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn policy_from(args: &[String]) -> planner::Policy {
    // `--prefer alpha=composite_white`, repeatable: rank routes whose policy
    // for that property matches above those whose policy differs (ADR-0012).
    let prefer = args.windows(2)
        .filter(|w| w[0] == "--prefer")
        .filter_map(|w| w[1].split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();
    planner::Policy {
        build_profile: opt(args, "--profile").unwrap_or_else(|| "lgpl".into()),
        allow_unverified: flag(args, "--allow-unverified"),
        prefer,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return usage();
    }
    let root = repo_root();
    let reg = match Registry::load(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to load registry: {e}");
            return ExitCode::FAILURE;
        }
    };

    match args[0].as_str() {
        "detect" => {
            let Some(path) = args.get(1) else { return usage() };
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => { eprintln!("{path}: {e}"); return ExitCode::FAILURE; }
            };
            let name = Path::new(path).file_name().map(|s| s.to_string_lossy().to_string());
            let d = detect::detect(&reg, &bytes, name.as_deref());
            println!("{}", serde_json::to_string_pretty(&d).unwrap());
            if d.extension_mismatch {
                eprintln!(
                    "\nwarning: content says {} but the extension says otherwise. \
                     The extension is never authoritative (ADR-0005).",
                    d.format
                );
            }
            ExitCode::SUCCESS
        }

        "plan" => {
            let (Some(from), Some(to)) = (args.get(1), args.get(2)) else { return usage() };
            let pol = policy_from(&args);
            let p = planner::Planner::new(&reg, &pol);
            eprintln!(
                "registry {} — {} of {} edges eligible (profile={}, unverified={})",
                reg.version, p.eligible_count(), reg.edges.len(),
                pol.build_profile, pol.allow_unverified
            );
            let k: usize = opt(&args, "-k").and_then(|v| v.parse().ok()).unwrap_or(1);
            if k > 1 {
                let plans = p.plan_ranked(from, to, k);
                if plans.is_empty() {
                    eprintln!("\nrefused: no route from {from} to {to}.");
                    return ExitCode::from(3);
                }
                eprintln!("\n{} distinct route(s) — deduplicated by what they cost you,\n\
                           not by which engines they use:\n", plans.len());
                for (i, pl) in plans.iter().enumerate() {
                    eprintln!("  [{}] {}", i + 1, pl.intent_label());
                    eprintln!("      route: {}", pl.hops.iter()
                        .map(|h| format!("{} --{}--> {}", h.from, h.engine, h.to))
                        .collect::<Vec<_>>().join("  |  "));
                    eprintln!("      cost:  {:.2}", pl.totals.cost);
                    eprintln!("      {}\n", pl.rationale);
                }
                println!("{}", serde_json::to_string_pretty(&plans).unwrap());
                return ExitCode::SUCCESS;
            }
            match p.plan(from, to) {
                Some(plan) => {
                    println!("{}", serde_json::to_string_pretty(&plan).unwrap());
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!(
                        "\nrefused: no route from {from} to {to}.\n\
                         This is a normal outcome, not an error. Either no engine can \
                         do it, every candidate route is unverified (try \
                         --allow-unverified), or the engines are excluded by the \
                         active build profile."
                    );
                    ExitCode::from(3)
                }
            }
        }

        "convert" => {
            let (Some(inp), Some(outp)) = (args.get(1), args.get(2)) else { return usage() };
            let quality: u8 = opt(&args, "--quality")
                .and_then(|q| q.parse().ok())
                .unwrap_or(85);
            let bytes = match std::fs::read(inp) {
                Ok(b) => b,
                Err(e) => { eprintln!("{inp}: {e}"); return ExitCode::FAILURE; }
            };
            let name = Path::new(inp).file_name().map(|s| s.to_string_lossy().to_string());
            let d = detect::detect(&reg, &bytes, name.as_deref());

            // Target comes from the OUTPUT extension, resolved through the
            // registry rather than guessed.
            let out_ext = Path::new(outp)
                .extension()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let Some(target) = reg.formats.iter()
                .find(|(_, f)| f.extensions.contains(&out_ext))
                .map(|(id, _)| id.clone())
            else {
                eprintln!("no registry format for output extension .{out_ext}");
                return ExitCode::from(3);
            };

            let pol = policy_from(&args);
            let pl = planner::Planner::new(&reg, &pol);
            let Some(plan) = pl.plan(&d.format, &target) else {
                // Say WHY when the route exists but is legally out of reach
                // under this profile (ADR-0007), rather than implying none does.
                let other = ["lgpl", "gpl"].into_iter()
                    .filter(|p| *p != pol.build_profile)
                    .find(|p| {
                        let alt = planner::Policy { build_profile: (*p).into(), ..policy_from(&args) };
                        planner::Planner::new(&reg, &alt).plan(&d.format, &target).is_some()
                    });
                match other {
                    Some(p) => eprintln!(
                        "refused: no route from {} to {target} under the `{}` build profile. \
                         One exists under `{p}` (--profile {p}); it uses an engine that \
                         profile allows and this one does not (ADR-0007).",
                        d.format, pol.build_profile
                    ),
                    None => eprintln!(
                        "refused: no route from {} to {target}. Try --allow-unverified \
                         if you are probing a new edge.",
                        d.format
                    ),
                }
                return ExitCode::from(3);
            };

            eprintln!("detected {} (by {}, confidence {:.2})", d.format, d.detected_by, d.confidence);
            eprintln!("route:   {}", plan.hops.iter()
                .map(|h| format!("{} --{}[{}]--> {}", h.from, h.engine, h.op, h.to))
                .collect::<Vec<_>>().join("  |  "));
            eprintln!("cost:    {:.2}", plan.totals.cost);
            eprintln!("effect:  {}", plan.rationale);
            if plan.totals.contains_synthesis {
                eprintln!("NOTE:    this route GENERATES content with a model (ADR-0009).");
            }

            match execute::execute(&plan, &bytes, quality) {
                Ok((out, receipt)) => {
                    if let Err(e) = std::fs::write(outp, &out) {
                        eprintln!("{outp}: {e}");
                        return ExitCode::FAILURE;
                    }
                    eprintln!(
                        "\nwrote {outp} ({} bytes, {:.1}% of input) in {} ms",
                        out.len(),
                        100.0 * out.len() as f64 / bytes.len().max(1) as f64,
                        receipt.wall_ms
                    );
                    if let Some(s) = receipt.ssim {
                        eprintln!("SSIM vs original: {s:.4}");
                    }
                    if let Some(why) = &receipt.unmeasured {
                        eprintln!("not measured: {why}");
                    }
                    let bad: Vec<_> = receipt.measurements.iter().filter(|m| m.contradicts).collect();
                    for m in &receipt.measurements {
                        eprintln!(
                            "  {:<12} declared={:<12} measured={:<12}{}",
                            m.property, m.declared, m.measured,
                            if m.contradicts { "  <-- CONTRADICTS REGISTRY" } else { "" }
                        );
                    }
                    if let Some(r) = opt(&args, "--receipt") {
                        let _ = std::fs::write(&r, serde_json::to_string_pretty(&receipt).unwrap());
                        eprintln!("receipt: {r}");
                    }
                    if !bad.is_empty() {
                        eprintln!(
                            "\n{} measurement(s) contradict the registry. That is a REGISTRY BUG \
                             for capability-cartographer, not a conversion failure.",
                            bad.len()
                        );
                        return ExitCode::from(4);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("conversion failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        "suggest" => {
            let Some(path) = args.get(1) else { return usage() };
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => { eprintln!("{path}: {e}"); return ExitCode::FAILURE; }
            };
            let name = Path::new(path).file_name().map(|s| s.to_string_lossy().to_string());
            let d = detect::detect(&reg, &bytes, name.as_deref());
            let pol = policy_from(&args);
            let p = planner::Planner::new(&reg, &pol);

            eprintln!("{} is {} ({} bytes)\n", path, d.format, bytes.len());
            eprintln!("It can become:\n");

            // For raster, the meaningful user choice is the TARGET, not the
            // route: "what should this become, and what will that cost me?"
            let mut options: Vec<(String, planner::Plan)> = reg.formats.keys()
                .filter(|t| **t != d.format)
                .filter_map(|t| p.plan(&d.format, t).map(|pl| (t.clone(), pl)))
                .collect();
            options.sort_by(|a, b| a.1.totals.cost.partial_cmp(&b.1.totals.cost)
                .unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));

            if options.is_empty() {
                eprintln!("  nothing — no verified route leaves {} yet.", d.format);
                eprintln!("  (try --allow-unverified to see claimed-but-unmeasured edges)");
                return ExitCode::from(3);
            }
            for (target, pl) in &options {
                eprintln!("  {:<14} cost {:>5.2}  {}", target, pl.totals.cost, pl.intent_label());
                eprintln!("  {:<14}             {}\n", "", pl.rationale);
            }
            ExitCode::SUCCESS
        }

        "clean" => {
            let (Some(inp), Some(outp)) = (args.get(1), args.get(2)) else { return usage() };
            let bytes = match std::fs::read(inp) {
                Ok(b) => b,
                Err(e) => { eprintln!("{inp}: {e}"); return ExitCode::FAILURE; }
            };
            let name = Path::new(inp).file_name().map(|s| s.to_string_lossy().to_string());
            let d = detect::detect(&reg, &bytes, name.as_deref());
            if d.detected_by == "unknown" {
                eprintln!(
                    "refused: cannot identify {inp} by content.\n\
                     Cleaning means parsing content and rebuilding it. An unidentified \
                     file cannot be parsed, so passing the bytes through unchanged would \
                     be the one genuinely dangerous outcome."
                );
                return ExitCode::from(3);
            }
            let dpi: u32 = opt(&args, "--dpi").and_then(|v| v.parse().ok()).unwrap_or(150);

            eprintln!("detected {} (by {}, confidence {:.2})", d.format, d.detected_by, d.confidence);
            if d.extension_mismatch {
                eprintln!("NOTE:    extension disagrees with content — cleaning by CONTENT type.");
            }

            match clean::clean(&bytes, &d.format, dpi) {
                Ok((out, report)) => {
                    if let Err(e) = std::fs::write(outp, &out) {
                        eprintln!("{outp}: {e}"); return ExitCode::FAILURE;
                    }
                    eprintln!("strategy {} ({} -> {} bytes, {} ms)",
                              report.strategy, report.input_bytes, report.output_bytes, report.wall_ms);
                    for inv in &report.invocations {
                        eprintln!("  ran: {inv}");
                    }
                    if report.removed.is_empty() {
                        eprintln!("\nno non-content data found");
                    } else {
                        eprintln!("\nremoved:");
                        for r in &report.removed { eprintln!("  - {r}"); }
                    }
                    if !report.properties_lost.is_empty() {
                        eprintln!("\nproperties destroyed: {}", report.properties_lost.join(", "));
                        eprintln!("(that is the COST of cleaning, not a bug — see docs/specs/loss-vector.md)");
                    }
                    let faithful = match report.sanitize.as_ref() {
                        Some(sr) => {
                            match (sr.content_ssim, sr.content_preserved) {
                                (Some(s), true) => eprintln!("content intact: SSIM {s:.4}"),
                                (Some(s), false) => eprintln!(
                                    "WARNING: content CHANGED, SSIM {s:.4} — the rebuild was not faithful"),
                                (None, _) => eprintln!(
                                    "WARNING: content not comparable (geometry changed)"),
                            }
                            sr.content_preserved
                        }
                        None => true,
                    };
                    if let Some(r) = opt(&args, "--report") {
                        let _ = std::fs::write(&r, serde_json::to_string_pretty(&report).unwrap());
                        eprintln!("report:  {r}");
                    }
                    // A rebuild that mangles the content is a failure, even
                    // though it is technically "clean".
                    if !faithful { return ExitCode::from(4); }
                    ExitCode::SUCCESS
                }
                Err(e) => { eprintln!("clean failed: {e}"); ExitCode::FAILURE }
            }
        }

        "export" => {
            let dataset = export::build(&reg);

            // Refuse to publish something structurally broken. A consumer
            // cannot tell a checked dataset from an unchecked one, so the
            // check has to happen here rather than being left to a habit.
            let problems = export::integrity_problems(&dataset);
            if !problems.is_empty() {
                eprintln!("refusing to export: {} integrity problem(s)", problems.len());
                for p in problems.iter().take(10) {
                    eprintln!("  x {p}");
                }
                eprintln!("\nThis is a REGISTRY bug. Run tools/validate_registry.py for the \
                           full rule set — this check covers only what a consumer of the \
                           published data depends on.");
                return ExitCode::from(4);
            }

            let json = serde_json::to_string_pretty(&dataset).unwrap();
            match opt(&args, "--out") {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, json.as_bytes()) {
                        eprintln!("{path}: {e}");
                        return ExitCode::FAILURE;
                    }
                    eprintln!(
                        "wrote {path} — schema {}, registry {}\n  \
                         {} formats, {} engines, {} edges ({} verified, {} claimed)\n  \
                         licence {}",
                        dataset.schema_version, dataset.registry_version,
                        dataset.counts.formats, dataset.counts.engines,
                        dataset.counts.edges_total, dataset.counts.edges_verified,
                        dataset.counts.edges_claimed, dataset.license
                    );
                    if dataset.counts.edges_verified == 0 {
                        eprintln!("\nWARNING: no verified edges. Consumers should treat \
                                   `claimed` entries as untested assertions.");
                    }
                }
                None => println!("{json}"),
            }
            ExitCode::SUCCESS
        }

        "graph" => {
            println!("registry {}", reg.version);
            println!("{} formats, {} engines, {} edges", reg.formats.len(), reg.engines.len(), reg.edges.len());
            for pol_name in ["lgpl", "gpl"] {
                let pol = planner::Policy { build_profile: pol_name.into(), ..Default::default() };
                let verified = planner::Planner::new(&reg, &pol).eligible_count();
                let pol_all = planner::Policy {
                    build_profile: pol_name.into(), allow_unverified: true, ..Default::default()
                };
                let all = planner::Planner::new(&reg, &pol_all).eligible_count();
                println!("  profile {pol_name:<5} {verified} verified edges eligible, {all} including unverified");
            }
            println!("\nedges:");
            for e in &reg.edges {
                println!("  [{:<9}] {:<9} {:>16} -> {:<16} loss: {}",
                    e.status, e.op, e.from, e.to, e.loss.explain());
            }
            ExitCode::SUCCESS
        }

        _ => usage(),
    }
}
