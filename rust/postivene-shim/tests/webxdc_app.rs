//! Running a webxdc app: what the loopback host serves, and to whom.
//!
//! The app is served rather than unpacked, so every file it loads is a
//! request that has to reach the core and come back as the right bytes
//! with the right headers -- and every request that is *not* the app's has
//! to be refused. Both halves are driven here over a real socket, against
//! the fake core, with the same `WebxdcApp` a page instantiates.
//!
//! The round trip is the point of the last part: an update posted by the
//! page reaches the core, the core says so as an event, and the object
//! that started the app has the new summary. That is the whole bridge, in
//! the direction that cannot be checked by reading the JavaScript.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::io::{Read, Write};
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
    Item {
        property string log: ''
        WebxdcApp {
            id: app
            account_id: 1
            message_id: 5
            onError: log += 'error:' + message + ';'
            onGone: log += 'gone;'
        }
        // Nothing can be asked of the core until it is up, and it may
        // well be up before this page exists -- which is why the same
        // step runs on the signal and on the way in.
        function open() {
            if (core.status === 'ready') {
                app.reload()
                app.start()
            }
        }
        Component.onCompleted: open()
        Connections {
            target: core
            onStatus_changed: open()
            onCore_event: app.handle_event(context_id, kind, payload_json)
        }
        function url() { return app.url }
        function info() {
            return app.name + '|' + app.summary + '|'
                   + (app.icon_path.length > 0 ? 'icon' : 'no-icon')
        }
        function stop() { app.stop(); return app.url }
        // What the page shows while an app is coming up: how much of
        // itself the app has asked for.
        function served() { app.poll(); return '' + app.served }
        function said() { return log }
    }
";

/// One request, one answer, one closed connection: what the host does.
/// `Err` is nothing listening, which is what a stopped app should be.
fn ask(authority: &str, request: &str) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| err.to_string())?;
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("write: {err}"))?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|err| format!("read: {err}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// A GET for `path` on the host, as a browser on the phone would send it.
fn get(authority: &str, path: &str) -> Result<String, String> {
    ask(
        authority,
        &format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"),
    )
}

/// The address and the token part of the URL the app was given.
fn split(url: &str) -> (String, String) {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    match rest.split_once('/') {
        Some((authority, path)) => (
            authority.to_string(),
            path.split('/').next().unwrap_or_default().to_string(),
        ),
        None => (rest.to_string(), String::new()),
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn an_app_is_served_to_itself_alone_and_its_updates_reach_the_chat() {
    let temp = std::env::temp_dir().join(format!("postivene-webxdc-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    std::fs::create_dir_all(&cache).expect("create cache dir");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // The app icon is written here rather than into the runner's own
        // cache, and a fresh directory is what proves it was written.
        std::env::set_var("XDG_CACHE_HOME", &cache);
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        let url = call!("url");
        record!("url", url.clone());
        record!("info", call!("info"));
        let (authority, token) = split(&url);
        if token.is_empty() {
            return;
        }

        record!(
            "index",
            get(&authority, &format!("/{token}/index.html")).unwrap_or_else(|err| err)
        );
        record!(
            "bridge",
            get(&authority, &format!("/{token}/webxdc.js")).unwrap_or_else(|err| err)
        );
        // Another app on the phone, guessing the port.
        record!(
            "no-token",
            get(&authority, "/index.html").unwrap_or_else(|err| err)
        );
        record!(
            "wrong-token",
            get(&authority, "/0123456789abcdef0123456789abcdef/index.html")
                .unwrap_or_else(|err| err)
        );
        // A page in the browser, which would arrive under its own name.
        record!(
            "wrong-host",
            ask(
                &authority,
                &format!(
                    "GET /{token}/index.html HTTP/1.1\r\nHost: webxdc.example\r\n\
                     Connection: close\r\n\r\n"
                ),
            )
            .unwrap_or_else(|err| err)
        );
        record!(
            "escape",
            get(&authority, &format!("/{token}/../../etc/passwd")).unwrap_or_else(|err| err)
        );
        record!(
            "missing",
            get(&authority, &format!("/{token}/nothing.js")).unwrap_or_else(|err| err)
        );

        // The app sending a move, the way `webxdc.js` does.
        let update = "{\"payload\":{\"move\":\"e4\"}}";
        record!(
            "send",
            ask(
                &authority,
                &format!(
                    "POST /{token}/webxdc-api/send HTTP/1.1\r\nHost: {authority}\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{update}",
                    update.len()
                ),
            )
            .unwrap_or_else(|err| err)
        );
        record!(
            "updates",
            get(&authority, &format!("/{token}/webxdc-api/updates?serial=0"))
                .unwrap_or_else(|err| err)
        );
        record!("served", call!("served"));
    });

    // The event the send produced has had a turn of the loop to arrive,
    // and the object reloads on it.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("after-update", call!("info"));
        let url = call!("url");
        record!("stopped-url", call!("stop"));
        let (authority, token) = split(&url);
        record!(
            "after-stop",
            get(&authority, &format!("/{token}/index.html")).unwrap_or_else(|err| err)
        );
        record!("served-after-stop", call!("served"));
        record!("log", call!("said"));
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

    assert!(
        value("url").starts_with("http://127.0.0.1:"),
        "the app was not served on the loopback interface. {context}"
    );
    // Nothing has been played yet, so the app says nothing about itself.
    assert_eq!(
        value("info"),
        "Checkers||icon",
        "the row's name, summary and icon did not come from the core. {context}"
    );

    let index = value("index");
    assert!(
        index.starts_with("HTTP/1.1 200 OK"),
        "the app's own page was not served. {context}"
    );
    assert!(
        index.contains("<script src=\"webxdc.js\"></script>"),
        "the API was not put in front of the app's page. {context}"
    );
    assert!(
        index.contains("board"),
        "the page served is not the one in the archive. {context}"
    );
    assert!(
        index.contains("Content-Security-Policy: default-src 'self'"),
        "the app was served without a policy confining it. {context}"
    );

    let bridge = value("bridge");
    assert!(
        bridge.starts_with("HTTP/1.1 200 OK") && bridge.contains("self@example.org"),
        "the API was not served with this account's own address in it. {context}"
    );
    assert!(
        !bridge.contains("__SELF_ADDR__"),
        "the API still has its placeholders in it. {context}"
    );

    for refused in ["no-token", "wrong-token", "wrong-host", "escape", "missing"] {
        assert!(
            value(refused).starts_with("HTTP/1.1 404"),
            "{refused} was answered rather than refused: {}. {context}",
            value(refused)
        );
    }

    assert!(
        value("send").starts_with("HTTP/1.1 204"),
        "the app could not send an update. {context}"
    );
    let updates = value("updates");
    assert!(
        updates.contains("\"payload\":{\"move\":\"e4\"}") && updates.contains("\"serial\":1"),
        "the update did not come back with a serial on it: {updates}. {context}"
    );
    assert_eq!(
        value("after-update"),
        "Checkers|1 move(s)|icon",
        "the app's own summary did not follow the update it sent. {context}"
    );

    assert_eq!(
        value("stopped-url"),
        "",
        "the app is still being served after it was closed. {context}"
    );
    assert!(
        !value("after-stop").starts_with("HTTP/1.1 200"),
        "a closed app still answers: {}. {context}",
        value("after-stop")
    );
    // Nine requests were made of the host above, refusals among them, and
    // the page's own account of what an app is doing has to count all of
    // them: it is what tells a phone the difference between an engine
    // that never asked for the app and an app that was served and drew
    // nothing.
    assert_eq!(
        value("served"),
        "9",
        "the host did not count what it answered. {context}"
    );
    assert_eq!(
        value("served-after-stop"),
        "0",
        "a stopped app still had a count of its own. {context}"
    );
    assert_eq!(value("log"), "", "the app reported: {}", value("log"));

    // The icon came out of the archive and was written where a row can
    // draw it from.
    let icons = cache.join("postivene/postivene/webxdc");
    assert!(
        icons.join("1-5.png").exists(),
        "the app's icon was not cached for its row to draw. {context}"
    );
}
