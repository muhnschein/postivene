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

        function onMessage_sent(sentAccountId, sentChatId, messageId) {
            if (sentAccountId === page.accountId && sentChatId === page.chatId) {
                textField.text = ""
            }
        }

        function onCore_event(contextId, kind, payloadJson) {
            if (contextId !== page.accountId) {
                return
            }
            if (kind === "IncomingMsg" || kind === "MsgsChanged") {
                // MsgsChanged carries chatId 0 when more than one chat is
                // affected; refresh for our chat or for "unspecified".
                var chatId = JSON.parse(payloadJson).chatId
                if (chatId === page.chatId || chatId === 0) {
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
                text: model.text
                wrapMode: Text.Wrap
                horizontalAlignment: model.is_outgoing ? Text.AlignRight : Text.AlignLeft
                color: model.is_outgoing ? Theme.highlightColor : Theme.primaryColor
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
