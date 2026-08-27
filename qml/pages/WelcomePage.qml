import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The first screen. Deliberately offers no address and no password: a new
 * Delta Chat user does not have either yet, and the reference client asks
 * for neither here. See docs/ONBOARDING.md.
 *
 * It doubles as the resume path -- if the core already has a configured
 * account, this page never really shows, it just hands over to the chat
 * list.
 */
Page {
    id: page

    // True until we know whether a configured account already exists, so
    // the buttons don't flash before an auto-resume takes over.
    property bool probing: true

    // The core may already be "ready" before this page's handlers exist,
    // in which case onStatus_changed never fires for us -- probe directly.
    Component.onCompleted: {
        if (core.status === "ready") {
            core.refresh_accounts()
        } else if (core.status.indexOf("error") === 0) {
            probing = false
        }
    }

    // NB: Sailfish is on Qt 5.6, where `Connections` only recognises
    // `onFoo:` script bindings -- the `function onFoo() {}` form is Qt 5.15+
    // and is silently treated as an ordinary function declaration that is
    // never connected. Parameters arrive under the names the shim declares,
    // i.e. snake_case. tests/qml_syntax.rs fails the build if the newer
    // form comes back.
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
            // Only the mailbox path is offered so far. "Add as second
            // device" and "restore from backup" are the reference client's
            // other two answers here and need shim work first
            // (docs/GAP-ANALYSIS.md); a button that does nothing would be
            // worse than one that isn't there yet.
            onClicked: pageStack.push(Qt.resolvedUrl("EmailLoginPage.qml"), {})
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: page.probing
        size: BusyIndicatorSize.Large
    }
}
