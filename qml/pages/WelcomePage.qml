import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The first screen: no address, no password -- a new Delta Chat user has
 * neither (docs/ONBOARDING.md). Also the resume path: with a configured
 * account it hands straight over to the chat list.
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
                core.start_account_io(first_configured_id)
                pageStack.replace(Qt.resolvedUrl("ChatListPage.qml"),
                                  { accountId: first_configured_id })
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
            text: qsTr("Create New Profile")
            enabled: core.status === "ready"
            onClicked: pageStack.push(Qt.resolvedUrl("CreateProfilePage.qml"), {})
        }

        Button {
            objectName: "existingProfileButton"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("I Already Have a Profile")
            enabled: core.status === "ready"
            // Only the mailbox path so far; second device and backup
            // restore need shim work (docs/GAP-ANALYSIS.md).
            onClicked: pageStack.push(Qt.resolvedUrl("EmailLoginPage.qml"), {})
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: page.probing
        size: BusyIndicatorSize.Large
    }
}
