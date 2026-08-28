import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Pick someone to talk to. Actions first, then known contacts -- the order
 * the reference client uses, because a new user has no contacts yet and the
 * actions are the way they get one.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        query: searchField.text
        onError: page.errorMessage = message
        // Every route ends the same way: open the chat that now exists.
        onChat_ready: pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: qsTr("Chat")
        })
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

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: contacts.rows

        header: Column {
            width: page.width

            PageHeader {
                title: qsTr("New Chat")
            }

            SearchField {
                id: searchField
                objectName: "searchField"
                width: parent.width
                placeholderText: qsTr("Search contacts")
            }

            Button {
                objectName: "newGroupButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("New Group")
                onClicked: pageStack.push(Qt.resolvedUrl("NewGroupPage.qml"),
                                          { accountId: page.accountId })
            }

            Button {
                objectName: "inviteButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Use an Invite Link")
                onClicked: pageStack.push(Qt.resolvedUrl("InvitePage.qml"),
                                          { accountId: page.accountId })
            }

            Button {
                objectName: "newContactButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("New Contact")
                onClicked: pageStack.push(Qt.resolvedUrl("NewContactPage.qml"),
                                          { accountId: page.accountId })
            }

            ErrorBanner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }

        delegate: ListItem {
            objectName: "contactRow"
            contentHeight: Theme.itemSizeSmall

            Column {
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }

                Label {
                    width: parent.width
                    // An address contact cannot be written to encrypted, so
                    // it carries the same letter mark the chat list uses.
                    text: model.is_key_contact ? model.display_name
                                               : "✉ " + model.display_name
                    truncationMode: TruncationMode.Fade
                }

                Label {
                    width: parent.width
                    text: model.address
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
            }

            onClicked: contacts.open_chat_with(model.contact_id)
        }

        ViewPlaceholder {
            enabled: contacts.count === 0
            text: qsTr("No contacts yet")
            hintText: qsTr("Add one by address, or share an invite link")
        }
    }
}
