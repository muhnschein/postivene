import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The first screen: no address, no password -- a new Delta Chat user has
 * neither (docs/PROJECT.md), so the one thing to do here is add a
 * profile. Also the resume path: with a configured account it hands
 * straight over to the chat list, on the profile the app was last
 * closed on.
 */
Page {
    id: page

    // Hides the buttons until we know whether an account exists.
    property bool probing: true

    // The core may be ready before the handler below exists.
    Component.onCompleted: {
        if (core.status === "ready") {
            core.refresh_accounts()
        } else if (core.status.indexOf("error") === 0) {
            probing = false
        }
    }

    // Qt 5.6 recognises only `onFoo:` bindings, and injects parameters
    // under the shim's snake_case names. tests/qml_syntax.rs enforces it.
    Connections {
        target: core

        onStatus_changed: {
            if (core.status === "ready") {
                core.refresh_accounts()
            } else if (core.status.indexOf("error") === 0) {
                page.probing = false
            }
        }

        onAccounts_refreshed: {
            if (configured_count > 0) {
                // Every profile, not only the one shown: each of them is
                // one people write to, and the cover counts them all.
                core.start_all_account_io()
                // The profile the app was closed on, which the core
                // remembers; the chat list tells it which on every open.
                pageStack.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                                       { accountId: resume_account_id })
            } else {
                page.probing = false
            }
        }

        onAccount_error: page.probing = false
    }

    Column {
        anchors.centerIn: parent
        width: parent.width
        spacing: Theme.paddingLarge
        visible: !page.probing

        PageHeader {
            title: qsTr("Postivene")
            description: qsTr("Secure decentralized chat")
        }

        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            text: core.status.indexOf("error") === 0
                  ? core.status
                  : qsTr("No phone number, no account with us: your profile lives on a mail server of your choosing.")
        }

        Button {
            objectName: "createProfileButton"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Add profile")
            enabled: core.status === "ready"
            onClicked: pageStack.push(Qt.resolvedUrl("AddProfileDialog.qml"), {})
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: page.probing
        size: BusyIndicatorSize.Large
    }
}
