import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Postivene 1.0

/*
 * Where an app comes from: the webxdc store, as a page.
 *
 * A file browser filtered to *.xdc was the first answer, and it is the
 * wrong one for the common case -- somebody who wants to play a game with
 * a friend has no .xdc on their phone yet and no idea where to get one.
 * deltachat-android opens the store instead, and so does this.
 *
 * The store is an ordinary website. What is not ordinary is what happens
 * when a link to an app is followed: the engine would download it into
 * the browser's world, where this app cannot reach it, so the navigation
 * is stopped and the file fetched through the core instead
 * (`WebxdcStore`). That is deltachat-android's shape too, done with the
 * one hook this WebView offers -- watching where it is going.
 *
 * Pushed by URL and reporting on `picked`, as the picker pages are and
 * for the same reason: `Sailfish.WebView` resolves only where the browser
 * engine is installed, and here that costs this page rather than the
 * conversation it was opened from.
 */
Page {
    id: page

    property int accountId

    /// An app was chosen and is on the phone at this path.
    signal picked(string path)

    /// Where the apps are. Delta Chat's own default store, which is what
    /// deltachat-android opens; it is not a setting here because there is
    /// nowhere yet to put one.
    readonly property string storeUrl: "https://webxdc.org/apps/"

    property string errorMessage: ""

    WebxdcStore {
        id: store
        objectName: "store"
        account_id: page.accountId
        onPicked: {
            page.picked(path)
            pageStack.pop()
        }
        onError: page.errorMessage = message
    }

    /// Take the app the reader tapped rather than letting the engine
    /// download it.
    ///
    /// The store's own links end in `.xdc`, and following one is a
    /// navigation before it is a download -- which is the moment this
    /// gets, and where deltachat-android does the same thing from
    /// `shouldOverrideUrlLoading`.
    function catchApp() {
        var here = "" + view.url
        var path = here.split("#")[0].split("?")[0]
        if (path.slice(-4).toLowerCase() !== ".xdc" || store.fetching) {
            return
        }
        view.stop()
        store.fetch(here)
    }

    /// An app already on the phone: sent by somebody, or downloaded
    /// before. The store is where one is found, not the only place one
    /// can be.
    function pickFromPhone() {
        // The empty properties are passed rather than left out: a stack
        // that declares both takes two, and the one-argument form does
        // not reach it. ProfilesPage learned the same thing about
        // replaceAbove.
        var picker = pageStack.push(Qt.resolvedUrl("AttachAppPage.qml"), {})
        if (picker) {
            picker.picked.connect(function (path) {
                page.picked(path)
                pageStack.pop(page)
            })
        }
    }

    // Not a PageHeader: the title is the store's own text. Empty until
    // the page says what it is called, which is why it has a name of its
    // own to fall back on.
    ConversationHeader {
        id: header
        objectName: "storeHeader"
        //: The heading over the webxdc app store.
        title: view.title.length > 0 ? view.title : qsTr("Apps")
    }

    WebView {
        id: view
        objectName: "storeView"
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: fromPhone.top
        }
        url: page.storeUrl
        onUrlChanged: page.catchApp()
    }

    // Below the store rather than in a pulley: a pulley wants a
    // SilicaFlickable around it, and a flickable around a WebView takes
    // the drags the page itself needs. It is also the way out when the
    // store cannot be reached at all.
    Button {
        id: fromPhone
        objectName: "fromPhoneButton"
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
            horizontalCenter: parent.horizontalCenter
        }
        //: Button under the app store: pick a .xdc file already on the phone.
        text: qsTr("From the phone")
        onClicked: page.pickFromPhone()
    }

    BusyIndicator {
        objectName: "storeBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: store.fetching
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
