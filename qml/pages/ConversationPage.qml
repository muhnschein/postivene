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

        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onMessage_sent: {
            if (account_id === page.accountId && chat_id === page.chatId) {
                textField.text = ""
            }
        }

        onCore_event: {
            if (context_id !== page.accountId) {
                return
            }
            // Delivery-state events update ticks; the others change
            // content.
            if (kind === "IncomingMsg" || kind === "MsgsChanged"
                    || kind === "MsgDelivered" || kind === "MsgRead"
                    || kind === "MsgFailed") {
                // MsgsChanged carries chatId 0 for "several chats".
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
            objectName: "messageRow"
            // Sized by its text, not fixed: a device message runs to a dozen
            // wrapped lines, and a fixed row height makes them overlap each
            // other and the header.
            contentHeight: Math.max(Theme.itemSizeSmall,
                                    messageLabel.implicitHeight + 2 * Theme.paddingMedium)

            Label {
                id: messageLabel
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                // A mail icon marks messages that were not encrypted and
                // signed. Outgoing messages get a DC_STATE_* suffix.
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
