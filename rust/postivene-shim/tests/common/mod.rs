//! Reading back what the fake core was asked.

// Not every test uses every helper.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Every recorded call, in order. A line that does not parse is a torn
/// write, not noise: fail rather than drop it and assert on a short list.
pub fn records(journal: &Path) -> Vec<Value> {
    std::fs::read_to_string(journal)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("journal line is not one JSON object ({err}): {line}"))
        })
        .collect()
}

/// Method name and params per call.
pub fn calls(journal: &Path) -> Vec<(String, Value)> {
    records(journal)
        .into_iter()
        .map(|call| {
            (
                call.get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                call.get("params").cloned().unwrap_or(Value::Null),
            )
        })
        .collect()
}

/// Method names only, in order.
pub fn methods(journal: &Path) -> Vec<String> {
    calls(journal).into_iter().map(|(name, _)| name).collect()
}

/// A journal path with nothing in it yet. The temp directory is keyed by
/// process id, and a recycled one otherwise leaves the last run's calls in
/// place -- which reads as this run having made them.
pub fn fresh_journal(temp: &Path) -> PathBuf {
    let journal = temp.join("journal.jsonl");
    let _ = std::fs::remove_file(&journal);
    journal
}

/// The stub Silica module tree, for `QmlEngine::add_import_path`.
pub fn stubs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/silica-stubs")
}

/// A `file://` URL for one of the app's pages.
pub fn page_url(name: &str) -> String {
    format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/pages")
            .join(name)
            .display()
    )
}

/// A copy of the app's QML tree with Silica's `EnterKey` attached property
/// stripped out of it.
///
/// `EnterKey` is what decides which key the virtual keyboard shows instead
/// of Return, and it has no stub: attached types cannot be written in QML,
/// and `qmetaobject` passes a null attached-properties function for every
/// type it registers. A page that uses one therefore cannot be loaded
/// headlessly at all -- the engine reports "Non-existent attached object"
/// and hands back nothing. Dropping the two lines that mention it leaves
/// the rest of the page exactly as shipped, which is what a test loading
/// this copy is asserting about.
///
/// The copy is keyed by process id and rebuilt each time, so it never
/// serves a stale page from an earlier run.
pub fn qml_tree_without_enter_key() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let target = std::env::temp_dir().join(format!("postivene-qml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&target);
    copy_qml_without_enter_key(&source, &target);
    target
}

/// Recursive half of [`qml_tree_without_enter_key`].
fn copy_qml_without_enter_key(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create the QML copy");
    for entry in std::fs::read_dir(source).expect("read the QML tree") {
        let entry = entry.expect("read a QML tree entry");
        let (from, to) = (entry.path(), target.join(entry.file_name()));
        if entry.file_type().expect("stat a QML tree entry").is_dir() {
            copy_qml_without_enter_key(&from, &to);
        } else if from.extension().is_some_and(|kind| kind == "qml") {
            let text = std::fs::read_to_string(&from).expect("read a QML file");
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("EnterKey."))
                .collect();
            std::fs::write(&to, kept.join("\n")).expect("write a QML file");
        } else {
            std::fs::copy(&from, &to).expect("copy a QML file");
        }
    }
}

/// A `file://` URL for a page inside a copied tree.
pub fn page_url_in(tree: &Path, name: &str) -> String {
    format!("file://{}", tree.join("pages").join(name).display())
}
