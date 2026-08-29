import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Pick a chat to send something into.
 *
 * The list comes from the core with DC_GCL_FOR_FORWARDING, so it leaves
 * out the chats a forward would be refused by rather than offering them
 * and failing afterwards.
 *
 * Reports its answer with a signal rather than acting itself: what happens
 * to the chosen chat is the caller's business, and a picker that forwarded
 * on its own could not be reused for anything else.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    /// The reader picked this chat.
    signal chatPicked(int chatId, string chatName)

    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: chats.query = searchField.text
    }

    ChatList {
        id: chats
        objectName: "pickerChats"
        account_id: page.accountId
        for_forwarding: true
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        onCore_event: chats.handle_event(context_id, kind, payload_json)
    }

    SilicaListView {
        id: listView
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        clip: true
        model: chats.rows

        header: Column {
            width: page.width

            PageHeader {
                title: qsTr("Forward to")
            }

            SearchField {
                id: searchField
                objectName: "pickerSearchField"
                width: parent.width
                placeholderText: qsTr("Search chats")
                onTextChanged: searchDebounce.restart()
            }
        }

        // No context menu: choosing is the only thing to do with a row.
        delegate: ListItem {
            objectName: "pickerRow"
            contentHeight: body.height

            ChatListDelegate {
                id: body
                width: parent.width
                chatName: model.name
                preview: model.preview
                previewSender: model.preview_sender
                lastUpdated: model.last_updated
                isEncrypted: model.is_encrypted
                isPinned: model.is_pinned
                isMuted: model.is_muted
                chatColor: model.color
                avatarPath: model.avatar_path
                // Not a chat being read: an unread badge here is noise.
                unreadCount: 0
            }

            onClicked: {
                page.chatPicked(model.chat_id, model.name)
                pageStack.pop()
            }
        }

        ViewPlaceholder {
            enabled: chats.count === 0
            text: qsTr("No chats to forward to")
        }
    }

    Banner {
        id: banner
        objectName: "errorBanner"
        anchors.bottom: parent.bottom
        width: parent.width
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
