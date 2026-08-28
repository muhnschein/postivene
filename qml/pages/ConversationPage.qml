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
        // What the reader can actually see decides what counts as read.
        reading_history: !page.readerIsLooking
        onError: page.errorMessage = message
        // Sending is its own answer to "have I read this": go to the
        // message just sent rather than counting it as one that was missed.
        onSent: {
            textField.text = ""
            page.replyBody = ""
            page.replyAuthor = ""
            listView.jumpToNewest()
        }
        onArrived: listView.noteArrivals(count)
    }

    // Everything that has to hold for an arriving message to count as seen:
    // the app is in front, this page is the one on screen, and the view is
    // at the newest message and not mid-gesture. `following` rather than
    // `stickToBottom` because the latter is only recomputed when a drag
    // ends, so it still reads true throughout a drag away from the bottom.
    // Any one of these false means a read receipt would be a lie.
    readonly property bool readerIsLooking:
        Qt.application.state === Qt.ApplicationActive
        && page.status === PageStatus.Active
        && listView.following

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

        // Reaching the newest message is what marks what is there read.
        onArrivedAtNewest: messages.mark_seen_all()
        onReplyRequested: {
            messages.quoted_message_id = messageId
            page.replyBody = body
            page.replyAuthor = author
        }
        onCopyRequested: {
            Clipboard.text = body
            notice.show(qsTr("Copied to clipboard"))
        }
        onDeleteRequested: messages.delete_message(messageId)
        onResendRequested: messages.resend_message(messageId)
    }

    // Only up when the reader has scrolled away from the newest message.
    JumpButton {
        objectName: "jumpButton"
        visible: !listView.stickToBottom
        count: listView.missedCount
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: banner.top
            bottomMargin: Theme.paddingMedium
        }
        onClicked: listView.jumpToNewest()
    }

    // Between the list and the field rather than over the list: it is
    // translucent, and the messages behind it showed through.
    Banner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
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
    ReplyBar {
        id: replyBar
        objectName: "replyBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        author: page.replyAuthor
        body: page.replyBody
        onCancelled: page.cancelReply()
    }

    // Says what just happened where the page has no state for it, such as
    // a message going to the clipboard.
    Banner {
        id: notice
        objectName: "notice"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: replyBar.top
        }
        onDismissed: notice.text = ""
    }

    Row {
        id: inputRow
        anchors {
            left: parent.left
            right: parent.right
            // The field carries its own inset on the left; without the
            // same on this side the send button sits nearer the edge.
            rightMargin: Theme.horizontalPageMargin
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
            // The bar is cleared from `onSent`, with the model's own copy:
            // clearing it here would drop the reply the reader chose on a
            // send that never happened.
            messages.send(textField.text)
        }
    }
}
