use std::fs;
use std::path::{Path, PathBuf};

const SPELLING_SEAM: &str = "crates/standout-render/src/template/spelling.rs";

#[test]
fn production_code_builds_environments_through_the_spelling_seam() {
    let root = workspace_root();
    let mut sources = Vec::new();
    collect_rust_sources(&root.join("crates"), &mut sources);
    assert!(
        sources.len() > 50,
        "source walk found only {} files — the guard is not looking where it thinks",
        sources.len()
    );

    let seam = root.join(SPELLING_SEAM);
    assert!(seam.is_file(), "{SPELLING_SEAM} moved; update this guard");

    let offenders: Vec<String> = sources
        .iter()
        .filter(|path| **path != seam)
        .filter(|path| {
            let source = fs::read_to_string(path).expect("source file is readable");
            strip_line_comments(&source).contains("Environment::new()")
        })
        .map(|path| display_path(&root, path))
        .collect();

    assert!(
        offenders.is_empty(),
        "these files call minijinja::Environment::new() directly, so their \
         environments render `True`/`False`/`None`; call \
         standout_render::template::new_environment() instead: {offenders:?}"
    );
}

#[test]
fn prose_naming_the_constructor_is_not_read_as_a_call() {
    let commented = "//! Production code does not call Environment::new().\n\
                     let x = 1; // ... unlike Environment::new()\n";
    assert!(!strip_line_comments(commented).contains("Environment::new()"));

    let call = "let env = Environment::new(); // the real thing\n";
    assert!(strip_line_comments(call).contains("Environment::new()"));
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(comment) => &line[..comment],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate sits at <workspace>/crates/<name>")
        .to_path_buf()
}

fn collect_rust_sources(crates_dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(crates_dir).expect("crates/ is readable") {
        let crate_dir = entry.expect("directory entry").path();
        if crate_dir.is_dir() {
            collect_recursively(&crate_dir.join("src"), out);
        }
    }
    // todo-example nests its crate one level deeper.
    for nested in ["todo-example"] {
        let dir = crates_dir.join(nested);
        if dir.is_dir() {
            for entry in fs::read_dir(&dir).expect("nested crate dir is readable") {
                let inner = entry.expect("directory entry").path();
                if inner.is_dir() {
                    collect_recursively(&inner.join("src"), out);
                }
            }
        }
    }
}

fn collect_recursively(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_recursively(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
