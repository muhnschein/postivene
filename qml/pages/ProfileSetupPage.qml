import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The work behind AddProfileDialog: the core asks the relay for an
 * account, stores what it answers, and starts IO; this page shows the
 * progress meanwhile and lands in the chat list when it is done. Cancel
 * and a failure both go back to the dialog, which still has what was
 * typed.
 *
 * Silica makes a dialog's accept destination the moment the dialog comes
 * on screen, so that it can be peeked at, and the dialog fills the name
 * and relay in on accept. So nothing happens here on creation: the work
 * starts when the page is the one on screen, which is after both.
 */
Page {
    id: page

    property string displayName
    property string providerQr

    // True while the core is working.
    property bool busy: false
    // Set once the core has been asked, so a second activation -- coming
    // back from a page pushed on top -- does not ask again.
    property bool started: false
    property int permille: 0
    property string errorMessage: ""

    // Cancel is the way back while the core is working: a swipe back
    // would drop this page, and with it the handler that opens the chat
    // list, leaving a profile made and nothing shown for it.
    backNavigation: !busy

    function begin() {
        if (page.started) {
            return
        }
        page.started = true
        page.busy = true
        page.permille = 0
        core.create_profile(displayName, providerQr)
    }

    onStatusChanged: {
        if (page.status === PageStatus.Active) {
            page.begin()
        }
    }
    // Pushed straight to the top, the page is active before the handler
    // above exists.
    Component.onCompleted: {
        if (page.status === PageStatus.Active) {
            page.begin()
        }
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        onProfile_created: {
            // Only while this page is waiting: an answer after cancel is
            // not this page's to act on.
            if (!page.busy) {
                return
            }
            page.busy = false
            pageStack.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                                   { accountId: account_id })
        }

        onProfile_error: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = message
        }

        // Not gated on `busy`: the core's last progress events can arrive
        // after the call that started them has already been answered.
        onConfigure_progress: page.permille = permille
    }

    PageHeader {
        id: header
        title: qsTr("Add profile")
    }

    Column {
        anchors.centerIn: parent
        width: page.width
        spacing: Theme.paddingLarge

        ProgressBar {
            objectName: "progressBar"
            width: parent.width
            visible: page.busy
            minimumValue: 0
            maximumValue: 1000
            value: page.permille
            label: qsTr("Contacting %1...").arg(page.providerQr.replace("dcaccount:", ""))
        }

        Button {
            objectName: "cancelButton"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: page.busy
            text: qsTr("Cancel")
            onClicked: {
                page.busy = false
                core.cancel_ongoing()
                pageStack.pop()
            }
        }

        Banner {
            objectName: "errorBanner"
            width: parent.width
            text: page.errorMessage
            onDismissed: page.errorMessage = ""
        }

        Button {
            objectName: "backButton"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: !page.busy
            text: qsTr("Back")
            onClicked: pageStack.pop()
        }
    }
}
