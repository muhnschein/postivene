//! Locates the server binary and the `qml/` directory, registers the
//! context properties, and hands off to the QML engine.

// See postivene-shim/src/lib.rs: qmetaobject's macros expand to references
// across that crate, and upstream's own examples use the glob.
#![allow(clippy::wildcard_imports)]

use std::path::PathBuf;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

/// CLI arg, then `POSTIVENE_RPC_SERVER`, then the bundled path, then
/// `PATH`.
///
/// Checked here rather than set by the desktop entry: Sailfish launches
/// `silica-qt5` apps through the invoker, which executes the binary itself,
/// so an `Exec=env FOO=bar app` wrapper is not reliably honoured.
fn rpc_server_path() -> String {
    if let Some(arg) = std::env::args().nth(1) {
        return arg;
    }
    if let Ok(env) = std::env::var("POSTIVENE_RPC_SERVER") {
        return env;
    }
    let bundled = PathBuf::from("/usr/libexec/postivene/deltachat-rpc-server");
    if bundled.is_file() {
        return bundled.to_string_lossy().into_owned();
    }
    "deltachat-rpc-server".to_string()
}

/// `POSTIVENE_QML_DIR`, then the installed path, then the source tree.
fn qml_dir() -> PathBuf {
    if let Ok(env) = std::env::var("POSTIVENE_QML_DIR") {
        return PathBuf::from(env);
    }
    let installed = PathBuf::from("/usr/share/postivene/qml");
    if installed.is_dir() {
        return installed;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml")
}

fn main() {
    let core = QObjectBox::new(DeltaChatCore::default());

    // A `QQuickView`, not a bare `QmlEngine`: Silica's `ApplicationWindow`
    // must be hosted in a view and shown, as `SailfishApp::createView()`
    // does. An engine alone loads the QML but never creates a window.
    let mut view = QQuickView::new();

    // Must exist before the QML is sourced, or bindings see undefined.
    view.engine()
        .set_object_property("core".into(), core.pinned());
    view.engine().set_property(
        "rpcServerPath".into(),
        QString::from(rpc_server_path()).into(),
    );

    let main_qml = qml_dir().join("postivene.qml");
    view.set_source(QString::from(main_qml.to_string_lossy().into_owned()));
    view.show();
    view.engine().exec();
}
