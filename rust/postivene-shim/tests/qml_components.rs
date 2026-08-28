//! The small pieces the conversation page is built from, loaded on their
//! own: the page itself cannot be, because Silica's `EnterKey` attached
//! property has no stub.

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

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function call(name, argument) { return '' + loader.item[name](argument) }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // Colours compare badly as strings; the alpha is what matters.
        function alphaOf(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property].a
        }
        function own(property) { return '' + loader.item[property] }
    }
";

fn component_url(name: &str) -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .join(name)
            .display()
    )
}

/// Long enough to overflow three lines even at the stub's small font; on a
/// phone it takes far less.
const LONG_BODY: &str = "a quoted message long enough to need three lines of its own in \
     the bar above the field, and then a good deal more after that, so that what does \
     not fit has to be cut off rather than run off the side of the screen, which is what \
     it did before -- the label had no wrapping at all, so a single long line simply ran \
     past the edge and out of sight, taking the rest of the quote with it, and none of \
     that is any use to someone trying to see which message they are about to answer";

// A script of timed steps, in the order they happen; splitting it would
// hide that order for no gain.
#[allow(clippy::too_many_lines)]
#[test]
fn the_reply_bar_wraps_the_jump_button_is_opaque_and_a_notice_is_quiet() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "bar-load",
            call!("load", QString::from(component_url("ReplyBar.qml")))
        );
        call!("set", QString::from("author"), QString::from("Ada"));
        call!("set", QString::from("body"), QString::from(LONG_BODY));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "bar-lines",
            call!(
                "get",
                QString::from("replyLabel"),
                QString::from("lineCount")
            )
        );
        record!(
            "bar-truncated",
            call!(
                "get",
                QString::from("replyLabel"),
                QString::from("truncated")
            )
        );
        record!("bar-height", call!("own", QString::from("height")));

        record!(
            "jump-load",
            call!("load", QString::from(component_url("JumpButton.qml")))
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "jump-opaque",
            call!("alphaOf", QString::from("jumpDisc"), QString::from("color"))
        );
        record!(
            "jump-icon",
            call!("get", QString::from("jumpIcon"), QString::from("source"))
        );
        record!(
            "badge-hidden",
            call!("get", QString::from("jumpBadge"), QString::from("visible"))
        );
        call!("set", QString::from("count"), 3.0);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "badge-shown",
            call!("get", QString::from("jumpBadge"), QString::from("visible"))
        );
        record!(
            "badge-text",
            call!(
                "get",
                QString::from("jumpBadgeLabel"),
                QString::from("text")
            )
        );

        record!(
            "banner-load",
            call!("load", QString::from(component_url("Banner.qml")))
        );
        call!("set", QString::from("tone"), QString::from("info"));
        call!(
            "call",
            QString::from("show"),
            QString::from("Copied to clipboard")
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "notice-text",
            call!("get", QString::from("errorLabel"), QString::from("text"))
        );
        record!("notice-shown", call!("own", QString::from("visible")));
        record!("notice-tone", call!("own", QString::from("tone")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Each piece does the one thing it is there for.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("bar-load"),
        "ok",
        "the reply bar did not load. {context}"
    );
    assert_eq!(
        value("bar-lines"),
        "3",
        "a long quote is not wrapped to three lines. {context}"
    );
    assert_eq!(
        value("bar-truncated"),
        "true",
        "a quote too long for three lines is not cut off. {context}"
    );
    assert!(
        value("bar-height").parse::<f64>().unwrap_or_default() > 0.0,
        "the bar has no height to wrap into. {context}"
    );

    assert_eq!(
        value("jump-load"),
        "ok",
        "the jump button did not load. {context}"
    );
    assert_eq!(
        value("jump-opaque"),
        "1",
        "the jump button is see-through, so the messages behind it show. {context}"
    );
    assert!(
        value("jump-icon").contains("icon-m-down"),
        "the jump button carries no chevron: {}. {context}",
        value("jump-icon")
    );
    assert_eq!(
        value("badge-hidden"),
        "false",
        "the button wore a badge with nothing to count. {context}"
    );
    assert_eq!(
        value("badge-shown"),
        "true",
        "messages arrived and the button did not say so. {context}"
    );
    assert_eq!(value("badge-text"), "3", "the badge miscounts. {context}");

    assert_eq!(
        value("banner-load"),
        "ok",
        "the banner did not load. {context}"
    );
    assert_eq!(
        value("notice-text"),
        "Copied to clipboard",
        "a notice does not say what happened. {context}"
    );
    assert_eq!(
        value("notice-shown"),
        "true",
        "a notice with something to say stayed hidden. {context}"
    );
    assert_eq!(
        value("notice-tone"),
        "info",
        "a confirmation is dressed as a failure. {context}"
    );
}
