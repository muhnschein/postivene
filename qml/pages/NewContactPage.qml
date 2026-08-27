import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0

/*
 * Add someone by email address.
 *
 * This is the unencrypted fallback: writing to an address the core has no
 * key for produces a plain email. The encrypted way in is an invite link
 * (docs/ONBOARDING.md), which is why this page says so.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
        onChat_ready: pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: nameField.text.length > 0 ? nameField.text : addressField.text
        })
    }

    function addContact() {
        if (addressField.text.indexOf("@") < 0) {
            page.errorMessage = qsTr("That is not an email address")
            return
        }
        page.errorMessage = ""
        contacts.start_chat_with_address(addressField.text, nameField.text)
    }

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("New Contact")
            }

            TextField {
                id: addressField
                objectName: "addressField"
                width: parent.width
                label: qsTr("Email address")
                placeholderText: label
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
            }

            TextField {
                id: nameField
                objectName: "nameField"
                width: parent.width
                label: qsTr("Name (optional)")
                placeholderText: label
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Messages to an address the app has no key for are sent as plain email. Scan or paste an invite link to chat encrypted.")
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
                objectName: "addButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Start Chat")
                enabled: addressField.text.length > 0
                onClicked: page.addContact()
            }
        }
    }
}
