//! Wires `postivene-shim`'s `DeltaChatCore` and list models into a
//! `QmlEngine` and loads the Silica UI from `qml/`.
//!
//! This binary itself has no protocol logic and barely any logic at all --
//! it locates the `deltachat-rpc-server` binary and the `qml/` directory,
//! registers a couple of context properties, and hands off to the QML
//! engine. Everything interesting lives in `postivene-shim` (Rust/Qt
//! bridge) and `qml/` (UI).

use std::path::PathBuf;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

/// Where to find `deltachat-rpc-server`: explicit CLI arg, then
/// `POSTIVENE_RPC_SERVER`, then the path the RPM bundles it at, then
/// `"deltachat-rpc-server"` resolved via `PATH`.
///
/// The installed path is checked *in the binary* rather than being handed
/// over by the desktop entry: Sailfish launches apps declaring
/// `X-Nemo-Application-Type=silica-qt5` through the invoker/booster, which
/// executes the application binary itself, so an `Exec=env FOO=bar app`
/// wrapper is not reliably honoured.
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

/// Where to find the `qml/` directory: `POSTIVENE_QML_DIR`, then the
/// OpenRepos/community-app install convention (`/usr/share/postivene/qml`
/// -- see `rpm/postivene.spec`), then the source tree location for local
/// development (`cargo run` from within this workspace).
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

    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core.pinned());
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(rpc_server_path()).into(),
    );

    let main_qml = qml_dir().join("postivene.qml");
    engine.load_file(QString::from(main_qml.to_string_lossy().into_owned()));
    engine.exec();
}
