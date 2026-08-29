import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Pick someone to talk to: known contacts in the list, and the ways to get
 * a new one in the pulley menu, as on the chat list.
 *
 * "New Contact" opens the invite page rather than an address form. An
 * address alone produces a chat that cannot be encrypted, and an invite is
 * how a Delta Chat contact is actually added; the address route lives on
 * behind the invite page for writing to someone who does not use Delta
 * Chat at all.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    // Not bound straight to the field: a round trip per keystroke asks the
    // core four times to type "anna", and only the last answer is wanted.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: contacts.query = searchField.text
    }

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
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

        PullDownMenu {
            MenuItem {
                objectName: "newGroupButton"
                text: qsTr("New Group")
                onClicked: pageStack.push(Qt.resolvedUrl("NewGroupPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                // An address alone produces a chat that cannot be
                // encrypted, so adding a contact starts from an invite --
                // which is how a Delta Chat contact is actually added.
                objectName: "newContactButton"
                text: qsTr("New Contact")
                onClicked: pageStack.push(Qt.resolvedUrl("InvitePage.qml"),
                                          { accountId: page.accountId })
            }
        }

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
                onTextChanged: searchDebounce.restart()
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }

        // No context menu: picking a contact is the only thing to do
        // with one here.
        delegate: ListItem {
            objectName: "contactRow"
            contentHeight: body.height

            ContactRow {
                id: body
                width: parent.width
                displayName: model.display_name
                address: model.address
                ownColor: model.color
                picturePath: model.avatar_path
                isKeyContact: model.is_key_contact
                isVerified: model.is_verified
            }

            onClicked: contacts.open_chat_with(model.contact_id)
        }

        ViewPlaceholder {
            enabled: contacts.count === 0
            text: qsTr("No contacts yet")
            hintText: qsTr("Add one with an invite link")
        }
    }
}
