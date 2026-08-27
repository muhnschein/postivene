import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    property bool busy: false
    // True while we don't yet know whether a configured account already
    // exists; keeps the login form hidden so it doesn't flash before an
    // auto-resume kicks in.
    property bool probing: true

    // The core may already be "ready" before this page's signal handlers
    // exist (start() is fast when the binary spawns cleanly), in which
    // case onStatus_changed would never fire for us -- probe explicitly.
    Component.onCompleted: {
        if (core.status === "ready") {
            core.refresh_accounts()
        } else if (core.status.indexOf("error") === 0) {
            probing = false
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        PullDownMenu {
            MenuItem {
                text: qsTr("Check health")
                onClicked: core.check_health()
            }
        }

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge
            visible: !page.probing

            PageHeader {
                title: qsTr("Postivene")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: core.status
                color: core.status.indexOf("error") === 0 ? Theme.errorColor : Theme.secondaryColor
            }

            TextField {
                id: addressField
                width: parent.width
                label: qsTr("Email address")
                placeholderText: label
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: passwordField.focus = true
            }

            PasswordField {
                id: passwordField
                width: parent.width
                label: qsTr("Password")
                placeholderText: label
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: page.beginLogin()
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Log in")
                enabled: !page.busy && core.status === "ready"
                    && addressField.text.length > 0 && passwordField.text.length > 0
                onClicked: page.beginLogin()
            }
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: page.busy || page.probing
        size: BusyIndicatorSize.Large
    }

    // NB: Sailfish is on Qt 5.6, where `Connections` only recognises
    // `onFoo:` script bindings -- the `function onFoo() {}` form is Qt 5.15+
    // and is silently treated as an ordinary function declaration that is
    // never connected. Signal parameters are injected by the names the
    // shim declares them with, i.e. snake_case (qmetaobject writes the Rust
    // identifiers into the metaobject verbatim).
    Connections {
        target: core

        // Once the core is up, look for an existing configured account so
        // a returning user lands in their chats, not the login form.
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

        onAccount_added: {
            core.configure_account(account_id, addressField.text, passwordField.text)
        }

        onAccount_error: {
            page.busy = false
            page.probing = false
        }

        onConfigure_done: {
            page.busy = false
            if (success) {
                pageStack.replace(Qt.resolvedUrl("ChatListPage.qml"), { accountId: account_id })
            }
        }
    }

    function beginLogin() {
        page.busy = true
        core.add_account()
    }
}
