//! The developer view: a recording started from the page writes what the
//! phone can say about the app, where an SSH session can pick it up.
//!
//! Driven through the page, with a `DevRecorder` of the probe's own in
//! place of the root window's, and the fake core running so there is a
//! second process to sample. What the recording wrote is then read back
//! from disk, which is the whole point of it.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// The page over a recorder of the probe's own, as the root window would
/// hand it one.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Postivene 1.0
    Item {
        DevRecorder { id: recorder }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { recorder: recorder })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function fill(name, text) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = text
            return 'ok'
        }
        // The heartbeat the root window would send while recording.
        function beat(times) {
            for (var i = 0; i < times; i++) { recorder.beat() }
            return 'ok'
        }
        function recording() { return '' + recorder.recording }
        function outputDir() { return '' + recorder.output_dir }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_recording_started_from_the_page_lands_on_disk() {
    let temp = std::env::temp_dir().join(format!("postivene-developer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let recordings = temp.join("recordings");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_RECORDINGS_DIR", &recordings);
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // The core is up by now, so the recording has a second process.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("DeveloperPage.qml")))
        );
        record!(
            "report",
            call!("get", QString::from("systemReport"), QString::from("text"))
        );
        record!("idle", call!("recording"));
        record!(
            "mark-idle",
            call!("get", QString::from("markButton"), QString::from("enabled"))
        );
        record!("start", call!("click", QString::from("recordButton")));
        record!("started", call!("recording"));
        record!(
            "button",
            call!("get", QString::from("recordButton"), QString::from("text"))
        );
        record!("dir", call!("outputDir"));
    });

    // A sample has landed; the reader marks what they are doing and asks
    // for a snapshot.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("beats", call!("beat", 7));
        record!(
            "typed",
            call!(
                "fill",
                QString::from("markField"),
                QString::from("opening the chat with the photos")
            )
        );
        record!("mark", call!("click", QString::from("markButton")));
        record!(
            "cleared",
            call!("get", QString::from("markField"), QString::from("text"))
        );
        record!("snapshot", call!("click", QString::from("snapshotButton")));
        record!(
            "status",
            call!("get", QString::from("status"), QString::from("text"))
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!(
            "summary",
            call!("get", QString::from("summary"), QString::from("text"))
        );
        record!("stop", call!("click", QString::from("recordButton")));
        record!("stopped", call!("recording"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    for key in ["Kernel:", "Seccomp:", "Landlock:", "LSM:", "Core pid:"] {
        assert!(
            value("report").contains(key),
            "the system report on the page lacks {key}. {context}"
        );
    }
    assert!(
        !value("report").contains("Core pid: not running"),
        "the core was not running, so the recording samples one process \
         where a phone has two. {context}"
    );
    assert_eq!(value("idle"), "false", "recording before asked. {context}");
    assert_eq!(
        value("mark-idle"),
        "false",
        "a mark can be made with nothing to put it on. {context}"
    );
    assert_eq!(value("started"), "true", "the button did not start a recording. {context}");
    assert_eq!(
        value("button"),
        "Stop recording",
        "the button does not offer to stop what it started. {context}"
    );
    let dir = std::path::PathBuf::from(value("dir"));
    assert!(
        dir.starts_with(&recordings),
        "the recording went somewhere other than the recordings directory: \
         {}. {context}",
        dir.display()
    );
    assert_eq!(value("cleared"), "", "the mark field kept its text. {context}");
    assert!(
        value("status").starts_with("Snapshot in "),
        "the snapshot did not say where it went. {context}"
    );
    assert!(
        value("summary").contains("fps") && value("summary").contains("core"),
        "the live line does not carry the frames and the core. {context}"
    );
    assert_eq!(value("stopped"), "false", "the button did not stop the recording. {context}");

    // What reached the disk.
    let system = std::fs::read_to_string(dir.join("system.txt")).expect("system.txt");
    assert!(system.contains("Landlock:"), "system.txt: {system}");
    let script = std::fs::read_to_string(dir.join("strace.sh")).expect("strace.sh");
    assert!(
        script.contains(&format!("APP={}", std::process::id())) && !script.contains("CORE=\"\""),
        "strace.sh does not name both processes: {script}"
    );
    for file in ["mounts.txt", "maps-app.txt", "maps-core.txt"] {
        assert!(dir.join(file).is_file(), "{file} was not written. {context}");
    }
    let timeline = std::fs::read_to_string(dir.join("timeline.tsv")).expect("timeline.tsv");
    let beats: u64 = timeline
        .lines()
        .filter(|line| line.split('\t').nth(1) == Some("frame"))
        .filter_map(|line| line.split('\t').nth(3)?.parse::<u64>().ok())
        .sum();
    assert!(beats >= 7, "the heartbeat's seven beats were not all counted: {timeline}");
    for needle in ["\tmem\tapp\t", "\tmem\tcore\t", "\tmark\topening the chat with the photos\n", "\tsnapshot\tsnapshot-1\n", "\tstop\n"] {
        assert!(timeline.contains(needle), "the timeline lacks {needle:?}: {timeline}");
    }
    for file in ["smaps-app.txt", "smaps-core.txt", "fd-core.txt", "threads-app.txt"] {
        assert!(
            std::fs::metadata(dir.join("snapshot-1").join(file)).map_or(0, |m| m.len()) > 0,
            "snapshot-1/{file} is missing or empty. {context}"
        );
    }
    let _ = std::fs::remove_dir_all(&temp);
}
