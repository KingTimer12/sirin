use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sirin_parser::stmt::Stmt;

#[derive(Debug)]
pub enum ModuleSource {
    Stdlib,
    Local(PathBuf),
}

/// Resolve a `use` path relative to the file that contains the `use` statement.
///
/// - `path[0] == "sirin"` → stdlib
/// - Otherwise → `<dir-of-from>/<segments...>.sn`
pub fn resolve(path: &[&str], from: &Path) -> Result<ModuleSource, String> {
    if path.first().copied() == Some("sirin") {
        return Ok(ModuleSource::Stdlib);
    }

    let dir = from.parent().unwrap_or(Path::new("."));
    let mut file_path = dir.to_path_buf();
    for segment in &path[..path.len().saturating_sub(1)] {
        file_path.push(segment);
    }
    let last = path.last().unwrap();
    file_path.push(format!("{}.sn", last));

    let canonical = file_path.canonicalize().map_err(|_| {
        format!(
            "módulo '{}' não encontrado em '{}'",
            path.join("."),
            file_path.display()
        )
    })?;

    Ok(ModuleSource::Local(canonical))
}

/// Collect transitive local module dependencies for `root` in topological order
/// (deepest dependencies first). `root` itself is NOT included in `ordered`.
pub fn collect_modules(
    root: &Path,
    stack: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
    ordered: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("não encontrado '{}': {}", root.display(), e))?;

    if visited.contains(&canonical) {
        return Ok(());
    }

    if stack.contains(&canonical) {
        let mut cycle: Vec<String> = stack.iter().map(|p| p.display().to_string()).collect();
        cycle.push(canonical.display().to_string());
        return Err(format!("ciclo de importação detectado: {}", cycle.join(" -> ")));
    }

    let src = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("não foi possível ler '{}': {}", canonical.display(), e))?;

    let use_paths = extract_use_paths(&src);

    stack.push(canonical.clone());

    for use_path in use_paths {
        let refs: Vec<&str> = use_path.iter().map(|s| s.as_str()).collect();
        match resolve(&refs, &canonical)? {
            ModuleSource::Local(dep_path) => {
                collect_modules(&dep_path, stack, visited, ordered)?;
            }
            ModuleSource::Stdlib => {}
        }
    }

    stack.pop();
    visited.insert(canonical.clone());
    ordered.push((canonical, src));

    Ok(())
}

/// Parse a source string and return the `use` paths as owned Strings.
fn extract_use_paths(src: &str) -> Vec<Vec<String>> {
    use chumsky::Parser;
    use chumsky::input::Input as _;
    use chumsky::span::SimpleSpan;

    let tokens = sirin_parser::lex(src);
    let eoi = SimpleSpan::from(src.len()..src.len());
    let stmts = match sirin_parser::parser::parser()
        .parse(tokens.as_slice().split_token_span(eoi))
        .into_result()
    {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmts
        .iter()
        .filter_map(|s| {
            if let Stmt::Use { path } = &s.node {
                Some(path.iter().map(|s| s.to_string()).collect())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "sirin_resolver_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn test_resolve_stdlib() {
        let from = PathBuf::from("/any/file.sn");
        let result = resolve(&["sirin", "io"], &from).unwrap();
        assert!(matches!(result, ModuleSource::Stdlib));
    }

    #[test]
    fn test_resolve_local() {
        let dir = tmp_dir();
        let math = dir.join("math.sn");
        fs::write(&math, "fn soma(a: int, b: int) -> int => a + b\n").unwrap();
        let from = dir.join("main.sn");
        fs::write(&from, "").unwrap();

        let result = resolve(&["math"], &from).unwrap();
        assert!(matches!(result, ModuleSource::Local(_)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_missing_error() {
        let dir = tmp_dir();
        let from = dir.join("main.sn");
        fs::write(&from, "").unwrap();

        let err = resolve(&["naoexiste"], &from).unwrap_err();
        assert!(err.contains("módulo 'naoexiste' não encontrado"), "{}", err);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_simple() {
        let dir = tmp_dir();
        let math = dir.join("math.sn");
        fs::write(&math, "fn soma(a: int, b: int) -> int => a + b\n").unwrap();
        let main = dir.join("main.sn");
        fs::write(&main, "use math\n").unwrap();

        let mut stack = vec![];
        let mut visited = HashSet::new();
        let mut ordered = vec![];

        // simulate: for each local use in main, collect deps
        let use_paths = extract_use_paths(&fs::read_to_string(&main).unwrap());
        for p in use_paths {
            let refs: Vec<&str> = p.iter().map(|s| s.as_str()).collect();
            if refs.first().copied() != Some("sirin") {
                let dep = match resolve(&refs, &main).unwrap() {
                    ModuleSource::Local(p) => p,
                    _ => continue,
                };
                collect_modules(&dep, &mut stack, &mut visited, &mut ordered).unwrap();
            }
        }

        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].0, math.canonicalize().unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_transitive() {
        let dir = tmp_dir();
        // C.sn (no deps)
        let c = dir.join("c.sn");
        fs::write(&c, "fn helper() -> int => 1\n").unwrap();
        // B.sn uses C
        let b = dir.join("b.sn");
        fs::write(&b, "use c\nfn mid() -> int => helper()\n").unwrap();
        // A.sn uses B (main)
        let a = dir.join("a.sn");
        fs::write(&a, "use b\n").unwrap();

        let mut stack = vec![];
        let mut visited = HashSet::new();
        let mut ordered = vec![];

        let use_paths = extract_use_paths(&fs::read_to_string(&a).unwrap());
        for p in use_paths {
            let refs: Vec<&str> = p.iter().map(|s| s.as_str()).collect();
            if refs.first().copied() != Some("sirin") {
                let dep = match resolve(&refs, &a).unwrap() {
                    ModuleSource::Local(p) => p,
                    _ => continue,
                };
                collect_modules(&dep, &mut stack, &mut visited, &mut ordered).unwrap();
            }
        }

        assert_eq!(ordered.len(), 2);
        // c.sn comes before b.sn (dependency first)
        assert!(ordered[0].0.ends_with("c.sn"));
        assert!(ordered[1].0.ends_with("b.sn"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_cycle_error() {
        let dir = tmp_dir();
        let a = dir.join("a.sn");
        let b = dir.join("b.sn");
        fs::write(&a, "use b\n").unwrap();
        fs::write(&b, "use a\n").unwrap();

        let mut stack = vec![];
        let mut visited = HashSet::new();
        let mut ordered = vec![];

        let dep = a.canonicalize().unwrap();
        let err = collect_modules(&dep, &mut stack, &mut visited, &mut ordered).unwrap_err();
        assert!(err.contains("ciclo"), "{}", err);
        let _ = fs::remove_dir_all(&dir);
    }
}
