import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Create a profile on a chatmail server: the user types a name, the server
 * mints the address and credentials (docs/ONBOARDING.md).
 *
 * The link field covers the QR scanner's ground without a camera -- every
 * dcaccount:, dclogin: and i.delta.chat payload is plain text.
 */
Page {
    id: page

    // Handed to the core; replaced when the user pastes a link.
    property string providerQr: core.default_provider_qr()
    property string providerLabel: hostOf(providerQr)
    // True from tapping create until the core answers.
    property bool busy: false
    property int permille: 0
    property string errorMessage: ""

    // For display only.
    function hostOf(qr) {
        var colon = qr.indexOf(":")
        return colon < 0 ? qr : qr.substring(colon + 1)
    }

    function beginCreate() {
        if (nameField.text.length === 0) {
            page.errorMessage = qsTr("Please enter a name")
            return
        }
        page.errorMessage = ""
        page.permille = 0
        page.busy = true
        core.create_profile(nameField.text, page.providerQr)
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

        // The core classifies the link; parsing it here would be the
        // protocol work docs/SCOPE.md rules out.
        onQr_checked: {
            if (kind === "account" || kind === "login") {
                page.providerQr = linkField.text
                page.providerLabel = page.hostOf(linkField.text)
                page.errorMessage = ""
            } else {
                page.errorMessage = qsTr("That link is not an invite or login code")
            }
        }

        onQr_error: page.errorMessage = message
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        PullDownMenu {
            MenuItem {
                objectName: "emailLoginMenuItem"
                text: qsTr("Log in to an email account")
                onClicked: pageStack.push(Qt.resolvedUrl("EmailLoginPage.qml"), {})
            }
            MenuItem {
                objectName: "useLinkMenuItem"
                text: qsTr("Use an invite or login link")
                onClicked: linkField.visible = true
            }
        }

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge
            visible: !page.busy

            PageHeader {
                title: qsTr("Create New Profile")
            }

            TextField {
                objectName: "nameField"
                id: nameField
                width: parent.width
                label: qsTr("Your name")
                placeholderText: label
                errorHighlight: page.errorMessage.length > 0 && text.length === 0
            }

            TextField {
                objectName: "linkField"
                id: linkField
                visible: false
                width: parent.width
                label: qsTr("Invite or login link")
                placeholderText: "dcaccount:..."
            }

            Label {
                objectName: "providerLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Your address will be created on %1.").arg(page.providerLabel)
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
                objectName: "createButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Agree & Create Profile")
                enabled: !page.busy && nameField.text.length > 0
                onClicked: page.beginCreate()
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
