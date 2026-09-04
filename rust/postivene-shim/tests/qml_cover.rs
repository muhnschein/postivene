//! The cover, with people in it, across two profiles.
//!
//! Two of its three states: the staggered grid of everyone in grey while
//! nothing is new, and whoever wrote lit up in colour where they stand in
//! it, with the count, once something is -- counted and drawn across both
//! profiles the fake core is told to have. The third state -- nobody yet
//! -- is `qml_cover_empty.rs`, since it takes a core seeded differently.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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

/// Loads the cover at a size, since the stub `CoverBackground` has none of
/// its own, and reads it back.
pub const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360 })
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
            return null
        }
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            return found
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The grid's cells, and how many of them are drawn in colour.
        function drawn() {
            return '' + allIn(loader.item, 'gridCell', []).length
        }
        function lit() {
            var cells = allIn(loader.item, 'gridCell', [])
            var total = 0
            for (var i = 0; i < cells.length; i++) {
                if (!cells[i].monochrome) { total += 1 }
            }
            return '' + total
        }
        // The leftmost cell: a shifted row starts half a cell off the edge.
        function leftmost() {
            var cells = allIn(loader.item, 'gridCell', [])
            var least = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].x < least) { least = cells[i].x }
            }
            return '' + least
        }
        function planned() { return '' + loader.item.cells.length }
        function people() { return '' + loader.item.people.length }
        // The lists, one per profile, in the profiles' order.
        function lists() {
            return '' + allIn(loader.item, 'coverChats', []).length
        }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_draws_everyone_and_lights_whoever_wrote() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-cover-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two configured profiles, each with the
    // fake's two chats.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("POSTIVENE_FAKE_ACCOUNTS", "1,2");
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("load", call!("load", QString::from(cover_url())));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("lists", call!("lists"));
        record!("brand", get!("brand", "text"));
        record!("subtitle", get!("subtitle", "text"));
        record!("empty-hidden", get!("emptyLabel", "visible"));
        record!("count-quiet", get!("unreadTotal", "text"));
        record!("count-shown", get!("unreadTotal", "visible"));
        record!("people", call!("people"));
        record!("planned", call!("planned"));
        record!("drawn-quiet", call!("drawn"));
        record!("lit-quiet", call!("lit"));
        record!("leftmost", call!("leftmost"));
        // Someone writes under each profile.
        record!("mark-first", call!("markUnread", 0, 1));
        record!("mark-second", call!("markUnread", 1, 2));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("count-loud", get!("unreadTotal", "text"));
        record!("drawn-loud", call!("drawn"));
        record!("planned-loud", call!("planned"));
        record!("lit-loud", call!("lit"));
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

    assert_eq!(value("load"), "ok", "the cover did not load. {context}");
    assert_eq!(
        value("lists"),
        "2",
        "the cover does not keep a list per profile. {context}"
    );
    assert_eq!(
        value("brand"),
        "Delta",
        "the cover does not name the app in its corner. {context}"
    );
    assert_eq!(
        value("subtitle"),
        "Messages",
        "the line under the name is not there. {context}"
    );
    assert_eq!(
        value("empty-hidden"),
        "false",
        "the cover says there is nobody while there are chats. {context}"
    );
    assert_eq!(
        value("count-shown"),
        "true",
        "the count is hidden with nothing unread; a zero says as much. {context}"
    );
    assert_eq!(
        value("count-quiet"),
        "0",
        "the count is not zero with nothing unread. {context}"
    );
    assert_eq!(
        value("people"),
        "4",
        "the people are not everyone across both profiles. {context}"
    );
    let planned: u32 = value("planned").parse().unwrap_or(0);
    assert!(
        planned >= 7,
        "the cover's shape gives the grid fewer than two rows. {context}"
    );
    assert_eq!(
        value("drawn-quiet"),
        planned.to_string(),
        "the grid is not filled from the people there are. {context}"
    );
    assert_eq!(
        value("lit-quiet"),
        "0",
        "someone is drawn in colour with nothing unread. {context}"
    );
    let leftmost: f64 = value("leftmost").parse().unwrap_or(0.0);
    assert!(
        leftmost < 0.0,
        "no row is shifted off the edge, so the rows do not stagger. {context}"
    );
    for label in ["mark-first", "mark-second"] {
        assert_eq!(
            value(label),
            "ok",
            "marking a chat unread failed. {context}"
        );
    }
    assert_eq!(
        value("count-loud"),
        "2",
        "the count is not every unread message across both profiles. {context}"
    );
    assert_eq!(
        value("drawn-loud"),
        value("planned-loud"),
        "the grid changed shape when someone wrote. {context}"
    );
    assert_eq!(
        value("lit-loud"),
        "2",
        "the two who wrote are not the two drawn in colour, once each, in \
         the grid. {context}"
    );
}

fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}
