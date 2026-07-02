//! `sirin run <file.sn>` — build and execute. With `--watch`, rebuild and
//! restart the process whenever the file or any local module it uses changes.
//! Change detection polls mtimes (300ms) — no platform watcher dependency.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, SystemTime};

use clap::ArgMatches;

use super::build::{BuildResult, try_build};

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap().clone();
    let watch = matches.get_flag("watch");

    if !watch {
        match try_build(&path) {
            Ok(res) => {
                let code = run_to_exit(&res.out_path);
                std::process::exit(code);
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    watch_loop(&path);
}

/// Absolute path to the produced binary, so `Command::new` never falls back to
/// PATH lookup (a relative `w` would otherwise run the system `w(1)`).
fn absolutize(bin: &PathBuf) -> PathBuf {
    std::fs::canonicalize(bin).unwrap_or_else(|_| {
        std::env::current_dir().map(|d| d.join(bin)).unwrap_or_else(|_| bin.clone())
    })
}

/// Run the binary in the foreground, inheriting stdio; return its exit code.
fn run_to_exit(bin: &PathBuf) -> i32 {
    let bin = absolutize(bin);
    match std::process::Command::new(&bin).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("error: cannot run `{}`: {}", bin.display(), e);
            1
        }
    }
}

fn spawn(bin: &PathBuf) -> Option<Child> {
    let bin = absolutize(bin);
    match std::process::Command::new(&bin).spawn() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("error: cannot run `{}`: {}", bin.display(), e);
            None
        }
    }
}

fn kill(child: &mut Option<Child>) {
    if let Some(c) = child.as_mut() {
        let _ = c.kill();
        let _ = c.wait();
    }
    *child = None;
}

fn mtimes(paths: &[PathBuf]) -> HashMap<PathBuf, SystemTime> {
    paths.iter()
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok().map(|t| (p.clone(), t)))
        .collect()
}

fn watch_loop(path: &str) {
    // The watch set starts as just the main file; every successful build
    // replaces it with main + resolved modules, so new `use`s are picked up.
    let mut watched: Vec<PathBuf> = PathBuf::from(path)
        .canonicalize()
        .map(|p| vec![p])
        .unwrap_or_else(|_| vec![PathBuf::from(path)]);
    let mut child: Option<Child> = None;

    // Ctrl-C: kill the child before exiting (avoid orphan servers).
    // Default signal handling already terminates us; the child dies with the
    // terminal group on Unix, so no handler dependency is needed.

    let mut rebuild = |watched: &mut Vec<PathBuf>, child: &mut Option<Child>| {
        kill(child);
        eprintln!("\x1b[2m[watch] building {}...\x1b[0m", path);
        match try_build(path) {
            Ok(BuildResult { out_path, sources }) => {
                *watched = sources;
                eprintln!("\x1b[2m[watch] running {}\x1b[0m", out_path.display());
                *child = spawn(&out_path);
            }
            Err(e) => {
                eprintln!("{}", e);
                eprintln!("\x1b[2m[watch] build failed — waiting for changes\x1b[0m");
            }
        }
    };

    rebuild(&mut watched, &mut child);
    let mut seen = mtimes(&watched);

    loop {
        std::thread::sleep(Duration::from_millis(300));

        // Report a child that exited on its own (once).
        if let Some(c) = child.as_mut() {
            if let Ok(Some(status)) = c.try_wait() {
                eprintln!("\x1b[2m[watch] process exited with {} — waiting for changes\x1b[0m", status);
                child = None;
            }
        }

        let now = mtimes(&watched);
        let changed = watched.iter().any(|p| seen.get(p) != now.get(p));
        if changed {
            rebuild(&mut watched, &mut child);
            seen = mtimes(&watched);
        } else {
            seen = now;
        }
    }
}
