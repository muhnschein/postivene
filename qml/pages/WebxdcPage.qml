import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Postivene 1.0

/*
 * A webxdc app, running.
 *
 * The app is a zip somebody sent, and it is *served* rather than
 * unpacked: the shim puts it on a loopback address of its own, reading
 * every file out of the archive through the core, and this points a
 * WebView at that address (`webxdc_host.rs`). The API the app talks to
 * the chat through is injected by the same host, so nothing here has to
 * reach into the page.
 *
 * A page of its own, pushed by URL, for the reason the picker pages are:
 * `Sailfish.WebView` is the one type in this tree that resolves only on a
 * release that ships the browser engine, and here that costs this page
 * rather than the conversation it was opened from.
 *
 * The object is what serves the app, so leaving the page stops it: the
 * page is destroyed on the way out, the object with it, and the socket
 * closes. Nothing has to be remembered to be cleaned up later.
 */
Page {
    id: page

    property int accountId
    property int messageId

    /// What the row said the app was called, until the core answers.
    property string appName: ""

    property string errorMessage: ""

    /// Everything the app is allowed to be: the directory it is served
    /// from, token and all. Empty until it is being served.
    readonly property string origin: app.url.length > 0
        ? app.url.substring(0, app.url.lastIndexOf("/") + 1) : ""

    /// Point the view somewhere. The view's `url` is assigned rather than
    /// bound, and only from here: an app that navigates itself away has
    /// to be put back, and a binding whose value has not changed cannot
    /// be made to fire again.
    function show(where) {
        view.url = where
    }

    Connections {
        target: app
        onUrl_changed: page.show(app.url)
    }

    /// Put the view back if the app navigates off the address it was
    /// served on.
    ///
    /// A webxdc has no network -- the policy it is served under permits
    /// this origin and nothing else -- but a policy does not stop the
    /// page itself moving, and an app that set `window.location` to the
    /// open web would be showing it inside a page wearing that app's
    /// name. So: it stays where it was put.
    function keepInside() {
        var here = "" + view.url
        if (page.origin.length === 0 || here.length === 0
                || here.indexOf(page.origin) === 0) {
            return
        }
        view.stop()
        page.show(app.url)
    }

    WebxdcApp {
        id: app
        objectName: "app"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
        // The message is gone -- deleted here or on another device --
        // so there is nothing left to run.
        onGone: pageStack.pop()
    }

    Connections {
        target: core
        onCore_event: app.handle_event(context_id, kind, payload_json)
        // An app started before the core was up has nothing to be served
        // from; the page opens as the chat list does and waits.
        onStatus_changed: page.run()
    }

    function run() {
        if (core.status === "ready") {
            app.reload()
            app.start()
        }
    }

    Component.onCompleted: page.run()

    // Leaving stops the app rather than leaving it served in the
    // background: `stop` here, and the object's own going for the way out
    // that never reaches this -- the whole stack being replaced.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            app.stop()
        }
    }

    // Not a PageHeader: the name is the app's own, and a header cannot be
    // told to draw what it is given as plain text.
    //
    // Nothing is offered on it. A pulley is the usual home for "where did
    // this come from", and there is nowhere here to put one: Silica's
    // pulley wants a SilicaFlickable around it, and a flickable around a
    // WebView takes the drags the app itself needs. The app's
    // `source_code_url` is on the shim object either way, for whatever
    // shows it (docs/PROJECT.md).
    ConversationHeader {
        id: header
        objectName: "webxdcHeader"
        title: app.name.length > 0 ? app.name : page.appName
    }

    // Where the app draws. Nothing is loaded until the host answers with
    // an address, and an empty URL is a WebView showing nothing rather
    // than a blank page from somewhere else.
    WebView {
        id: view
        objectName: "webxdcView"
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        active: app.url.length > 0
        onUrlChanged: page.keepInside()
    }

    BusyIndicator {
        objectName: "webxdcBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: app.url.length === 0 && page.errorMessage.length === 0
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
