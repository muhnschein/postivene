//! Guards the QML files against syntax that only exists in Qt versions
//! newer than the one Sailfish ships.
//!
//! This is a plain text scan on purpose: a *behavioural* test cannot catch
//! the regression, because host Qt (5.15) happily accepts the newer form
//! and merely warns. On device (Qt 5.6) the same file loads without any
//! error and the handlers simply never fire.

use std::fs;
use std::path::{Path, PathBuf};

fn qml_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "qml") {
                out.push(path);
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    files
}

#[test]
fn qml_avoids_qt_5_15_only_signal_handler_syntax() {
    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read qml");
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("function on") {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "`function onFoo() {{ ... }}` inside Connections is Qt 5.15+ syntax. \
         Sailfish runs Qt 5.6, where it is not an error but is never connected, \
         so the handler silently never runs. Use `onFoo: {{ ... }}` with the \
         shim's snake_case parameter names instead.\n  {}",
        offenders.join("\n  ")
    );
}
