import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * An existing mailbox as this profile's transport. One level in, where the
 * reference client keeps it: most Delta Chat users never type an IMAP
 * password (docs/ONBOARDING.md). Everything but address and password
 * autoconfigures.
 */
Page {
    id: page

    property bool busy: false
    property int permille: 0
    property string errorMessage: ""

    function beginLogin() {
        if (nameField.text.length === 0 || addressField.text.length === 0
                || passwordField.text.length === 0) {
            page.errorMessage = qsTr("Name, address and password are all needed")
            return
        }
        page.errorMessage = ""
        page.permille = 0
        page.busy = true
        core.create_profile_with_email(nameField.text, addressField.text,
                                       passwordField.text)
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        onProfile_created: {
            page.busy = false
            pageStack.replace(Qt.resolvedUrl("ChatListPage.qml"),
                              { accountId: account_id })
        }

        onProfile_error: {
            page.busy = false
            page.errorMessage = message
        }

        onConfigure_progress: page.permille = permille
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge
            visible: !page.busy

            PageHeader {
                title: qsTr("Log In")
            }

            TextField {
                objectName: "nameField"
                id: nameField
                width: parent.width
                label: qsTr("Your name")
                placeholderText: label
            }

            TextField {
                objectName: "addressField"
                id: addressField
                width: parent.width
                label: qsTr("Email address")
                placeholderText: label
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
            }

            PasswordField {
                objectName: "passwordField"
                id: passwordField
                width: parent.width
                label: qsTr("Password")
                placeholderText: label
            }

            Label {
                objectName: "errorLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.errorMessage.length > 0
                color: Theme.errorColor
                text: page.errorMessage
            }

            Button {
                objectName: "loginButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Log In")
                enabled: !page.busy
                onClicked: page.beginLogin()
            }
        }

        Column {
            anchors.centerIn: parent
            width: page.width
            spacing: Theme.paddingLarge
            visible: page.busy

            ProgressBar {
                objectName: "progressBar"
                width: parent.width
                minimumValue: 0
                maximumValue: 1000
                value: page.permille
                label: qsTr("One moment...")
            }

            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Cancel")
                onClicked: {
                    page.busy = false
                    core.cancel_ongoing()
                }
            }
        }
    }
}
