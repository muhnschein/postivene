import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * One conversation. The messages come from a ChatMessages instance owned by
 * this page, so a second open conversation cannot reset this one's model.
 */
Page {
    id: page

    property int accountId
    property int chatId
    property string chatName

    // Seconds east of UTC: the model groups messages by local day and has
    // no timezone of its own.
    property int utcOffset: -(new Date()).getTimezoneOffset() * 60

    ChatMessages {
        id: messages
        objectName: "messages"
        account_id: page.accountId
        chat_id: page.chatId
        utc_offset: page.utcOffset
        onError: page.errorMessage = message
        onSent: textField.text = ""
    }

    property string errorMessage: ""
    readonly property string coreStoppedMessage:
        qsTr("Lost the connection to the Delta Chat core. Restart Postivene.")

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        // The model ignores events for other accounts and chats itself.
        onCore_event: messages.handle_event(context_id, kind, payload_json)
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                messages.reload()
            }
        }
        onCore_error: page.errorMessage = message
    }

    // What the reader picked Reply on, for the bar above the field. The id
    // itself lives on the model, which is what the send reads.
    property string replyBody: ""
    property string replyAuthor: ""

    function cancelReply() {
        messages.quoted_message_id = 0
        page.replyBody = ""
        page.replyAuthor = ""
    }

    ConversationList {
        id: listView
        objectName: "messageList"
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        model: messages.rows
        // The model's own count, which changes when a row arrives rather
        // than when the view gets round to showing it.
        messageCount: messages.count
        title: page.chatName
        utcOffset: page.utcOffset
        showSender: messages.is_group
        placeholderText: qsTr("No messages yet")

        onReplyRequested: {
            messages.quoted_message_id = messageId
            page.replyBody = body
            page.replyAuthor = author
        }
        onCopyRequested: Clipboard.text = body
        onDeleteRequested: messages.delete_message(messageId)
        onResendRequested: messages.resend_message(messageId)
    }

    // Only up when the reader has scrolled away from the newest message.
    Button {
        objectName: "jumpButton"
        visible: !listView.stickToBottom
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: banner.top
            bottomMargin: Theme.paddingMedium
        }
        text: listView.missedCount > 0
              ? qsTr("%n new message(s)", "", listView.missedCount)
              : qsTr("Newest")
        onClicked: listView.jumpToNewest()
    }

    // Between the list and the field rather than over the list: it is
    // translucent, and the messages behind it showed through.
    ErrorBanner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: replyBar.top
        }
        // A dead core outranks whatever failed before it, and a page opened
        // after it died never saw the transition -- so read the status
        // rather than waiting for it to change.
        text: core.status === "stopped" ? page.coreStoppedMessage : page.errorMessage
        // That one does not fix itself, so it stays put.
        timeout: core.status === "stopped" ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    // What the next send replies to, and a way out of replying.
    Item {
        id: replyBar
        objectName: "replyBar"
        visible: messages.quoted_message_id > 0
        height: visible ? replyLabel.height + 2 * Theme.paddingSmall : 0
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }

        Label {
            id: replyLabel
            objectName: "replyLabel"
            anchors.verticalCenter: parent.verticalCenter
            x: Theme.horizontalPageMargin
            width: parent.width - x - cancelReplyButton.width - Theme.paddingMedium
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: qsTr("Replying to %1: %2").arg(page.replyAuthor).arg(page.replyBody)
        }

        IconButton {
            id: cancelReplyButton
            objectName: "cancelReplyButton"
            anchors {
                verticalCenter: parent.verticalCenter
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
            }
            icon.source: "image://theme/icon-m-clear"
            onClicked: page.cancelReply()
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
            objectName: "messageField"
            width: parent.width - sendButton.width
            placeholderText: qsTr("Message")
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: page.sendCurrentText()
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            icon.source: "image://theme/icon-m-send"
            onClicked: page.sendCurrentText()
        }
    }

    function sendCurrentText() {
        if (textField.text.length > 0) {
            page.errorMessage = ""
            // The model clears the quote itself; this is the bar's copy.
            messages.send(textField.text)
            page.replyBody = ""
            page.replyAuthor = ""
        }
    }
}
