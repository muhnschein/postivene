import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    property int accountId
    property int chatId
    property string chatName

    Component.onCompleted: core.open_chat(accountId, chatId)

    Connections {
        target: core

        // Qt 5.6 handler syntax with the shim's snake_case parameter
        // names -- see the note in SetupPage.qml.
        onMessage_sent: {
            if (account_id === page.accountId && chat_id === page.chatId) {
                textField.text = ""
            }
        }

        onCore_event: {
            if (context_id !== page.accountId) {
                return
            }
            // MsgDelivered/MsgRead/MsgFailed update the delivery-state
            // ticks on outgoing messages; the others add/change content.
            if (kind === "IncomingMsg" || kind === "MsgsChanged"
                    || kind === "MsgDelivered" || kind === "MsgRead"
                    || kind === "MsgFailed") {
                // MsgsChanged carries chatId 0 when more than one chat is
                // affected; refresh for our chat or for "unspecified".
                var eventChatId = JSON.parse(payload_json).chatId
                if (eventChatId === page.chatId || eventChatId === 0) {
                    core.open_chat(page.accountId, page.chatId)
                }
            }
        }
    }

    SilicaListView {
        id: listView
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        model: core.message_list

        header: PageHeader {
            title: page.chatName
        }

        delegate: ListItem {
            contentHeight: Theme.itemSizeSmall

            Label {
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                // Upstream guidance: mark messages that were NOT correctly
                // encrypted & signed (show_padlock false) with a small
                // email icon; encrypted is the unmarked normal case.
                // Outgoing messages carry a delivery-state suffix (the
                // DC_STATE_* constants: 20 pending, 24 failed,
                // 26 delivered, 28 read).
                text: (model.show_padlock ? "" : "✉ ") + model.text
                      + (model.is_outgoing ? " " + stateMark(model.state) : "")
                wrapMode: Text.Wrap
                horizontalAlignment: model.is_outgoing ? Text.AlignRight : Text.AlignLeft
                color: model.is_outgoing ? Theme.highlightColor : Theme.primaryColor

                function stateMark(state) {
                    if (state === 28) return "✓✓"
                    if (state === 26) return "✓"
                    if (state === 24) return "✗"
                    if (state === 20) return "…"
                    return ""
                }
            }
        }

        ViewPlaceholder {
            enabled: listView.count === 0
            text: qsTr("No messages yet")
        }
    }

    Row {
        id: inputRow
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        spacing: Theme.paddingSmall

        TextField {
            id: textField
            width: parent.width - sendButton.width
            placeholderText: qsTr("Message")
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: page.sendCurrentText()
        }

        IconButton {
            id: sendButton
            icon.source: "image://theme/icon-m-send"
            onClicked: page.sendCurrentText()
        }
    }

    function sendCurrentText() {
        if (textField.text.length > 0) {
            core.send_text(accountId, chatId, textField.text)
        }
    }
}
