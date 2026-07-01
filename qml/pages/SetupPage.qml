import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    property bool busy: false

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
        running: page.busy
        size: BusyIndicatorSize.Large
    }

    Connections {
        target: core

        function onAccount_added(accountId) {
            core.configure_account(accountId, addressField.text, passwordField.text)
        }

        function onAccount_error(message) {
            page.busy = false
        }

        function onConfigure_done(accountId, success, error) {
            page.busy = false
            if (success) {
                pageStack.replace(Qt.resolvedUrl("ChatListPage.qml"), { accountId: accountId })
            }
        }
    }

    function beginLogin() {
        page.busy = true
        core.add_account()
    }
}
