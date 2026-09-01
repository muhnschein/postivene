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
    /// A message a search found in this chat, to open at rather than at
    /// the newest. 0 opens the chat normally.
    property int findMessageId: 0

    ChatMessages {
        id: messages
        objectName: "messages"
        account_id: page.accountId
        // What the reader can actually see decides what counts as read.
        reading_history: !page.readerIsLooking
        onError: page.errorMessage = message
        // Sending is its own answer to "have I read this": go to the
        // message just sent rather than counting it as one that was missed.
        onSent: {
            textField.text = ""
            page.replyBody = ""
            page.replyAuthor = ""
            page.attachmentPath = ""
            listView.jumpToNewest()
        }
        onArrived: listView.noteArrivals(count)
    }

    // The chat is handed over as the page is built, so a prefetched one is
    // already in the model before the transition starts and the page comes
    // in with its messages rather than filling in behind itself.
    //
    // This used to wait for PageStatus.Active. It had to: a chat was
    // fetched whole, and building every row of a long history in one go on
    // the Qt thread froze the transition. A chat now opens on one page of
    // fifty, and the prefetch has usually built those rows already -- the
    // handover is then a move, with no core round trip in it at all.
    //
    // In `Component.onCompleted` rather than a binding on the declaration
    // above, because the order matters: this must run after
    // `reading_history` has been bound, or the model would see the default
    // `false`, take it for a reader looking at the screen, and mark the
    // chat read before the page is even on it.
    Component.onCompleted: messages.chat_id = page.chatId

    // Where a search result lands. The row cannot be looked up until the
    // fetch has finished, so this waits for the model to say so rather
    // than for the page: the two are not the same moment. And a chat opens
    // on its newest page, so the message a search found may not be loaded
    // at all -- `reveal` steps back until it is and then says where it
    // went, which is why this is two handlers rather than one lookup.
    Connections {
        target: messages
        onLoaded_changed: {
            if (messages.loaded && page.findMessageId !== 0) {
                messages.reveal(page.findMessageId)
            }
        }
        onRevealed: {
            if (row >= 0) {
                listView.foundMessageId = message_id
                listView.jumpToRow(row)
                foundFlash.restart()
            }
            // Once is enough, found or not: a later reload must not drag
            // the reader back off whatever they have scrolled to since.
            page.findMessageId = 0
        }
        // Rows have gone in above the ones on screen; put the view back.
        onOlder_loaded: listView.olderLoaded(count)
        // The loaded messages were replaced with a page from somewhere else
        // in the chat. `row` is where in it the reader was going.
        onWindow_moved: {
            if (messages.has_newer) {
                listView.jumpToRow(row)
            } else {
                // Back at the end of the chat, so this is the ordinary
                // "newest message" case, arrivals and all.
                listView.jumpToNewest()
            }
        }
    }

    // The flash says "this one" and then gets out of the way.
    Timer {
        id: foundFlash
        interval: 4000
        onTriggered: listView.foundMessageId = 0
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
    // Three states, not two: the core going away is now something the app
    // does something about, and a banner that says "restart Postivene"
    // while Postivene is already fixing it is worse than none.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting...")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Postivene.")
          : ""

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

    // The file the next send will carry, empty for none. One per message,
    // which is the core's own shape -- see ChatMessages.send_file.
    property string attachmentPath: ""
    readonly property string attachmentName: page.displayName(page.attachmentPath)

    // Decoded for the same reason the shim decodes the path it sends: a
    // picker that hands back a URL escapes the spaces, and the bar should
    // name the file the way the reader does. An escape that does not decode
    // is a name with a literal '%' in it.
    function displayName(path) {
        var name = path.substring(path.lastIndexOf('/') + 1)
        try {
            return decodeURIComponent(name)
        } catch (error) {
            return name
        }
    }

    // Both pickers report back here rather than sending, so a picked file
    // can still be cancelled, and so a caption can be typed after choosing.
    function attach(path) {
        if (path && path.length > 0) {
            page.attachmentPath = path
        }
    }

    // Pushed by URL and connected to, the way forwarding pushes
    // ChatPickerPage: the picker pages are the only files that name a
    // `Sailfish.Pickers` type, so a type that is not there costs one
    // button rather than the whole conversation.
    function pickWith(pageName) {
        var picker = pageStack.push(Qt.resolvedUrl(pageName))
        picker.picked.connect(page.attach)
    }

    // Outside the list rather than its `header`, so it stays put: a
    // header scrolls with the content, and in a long conversation the
    // name of whoever you are talking to disappears off the top.
    PageHeader {
        id: conversationHeader
        objectName: "conversationHeader"
        title: page.chatName
    }

    ConversationList {
        loaded: messages.loaded
        id: listView
        objectName: "messageList"
        anchors {
            top: conversationHeader.bottom
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        model: messages.rows
        // The model's own count, which changes when a row arrives rather
        // than when the view gets round to showing it.
        messageCount: messages.count
        hasOlder: messages.has_older
        loadingOlder: messages.loading_older
        hasNewer: messages.has_newer
        loadingNewer: messages.loading_newer
        loadingWindow: messages.loading_window
        onOlderRequested: messages.load_older()
        onNewerRequested: messages.load_newer()
        onOldestRequested: messages.jump_oldest()
        onNewestRequested: messages.jump_newest()
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
        onForwardRequested: {
            // The picker reports back rather than acting, so the
            // message id is captured here where it is still valid.
            var travelling = messageId
            var picker = pageStack.push(
                Qt.resolvedUrl("ChatPickerPage.qml"),
                { accountId: page.accountId })
            picker.chatPicked.connect(function(chatId) {
                messages.forward_to(travelling, chatId)
            })
        }
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
        // The core's own state outranks whatever failed before it, and a
        // page opened after it went away never saw the transition -- so
        // read the status rather than waiting for it to change.
        text: page.coreStatusMessage.length > 0
              ? page.coreStatusMessage : page.errorMessage
        // Neither clears itself: reconnecting ends when the status says so,
        // and stopped does not end at all.
        timeout: page.coreStatusMessage.length > 0 ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    // What the next send replies to, and a way out of replying.
    ReplyBar {
        id: replyBar
        objectName: "replyBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: attachmentBar.top
        }
        author: page.replyAuthor
        body: page.replyBody
        onCancelled: page.cancelReply()
    }

    // Directly above the field, below the reply bar: reading downwards,
    // the two bars are the message about to be sent, in the order its
    // parts appear in it.
    AttachmentBar {
        id: attachmentBar
        objectName: "attachmentBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        filePath: page.attachmentPath
        fileName: page.attachmentName
        onCancelled: page.attachmentPath = ""
    }

    // Says what just happened where the page has no state for it, such as
    // a message going to the clipboard.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
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
            width: parent.width - attachButton.width - sendButton.width
            //: Message field placeholder. Also the prompt for the caption
            //: on a message that is carrying a file.
            placeholderText: page.attachmentPath.length > 0
                             ? qsTr("Caption") : qsTr("Message")
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: page.sendCurrentText()
        }

        AttachButton {
            id: attachButton
            objectName: "attachButton"
            onPhotoRequested: page.pickWith("AttachPhotoPage.qml")
            onVideoRequested: page.pickWith("AttachVideoPage.qml")
            onAudioRequested: page.pickWith("AttachAudioPage.qml")
            onFileRequested: page.pickWith("AttachFilePage.qml")
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            // Hidden rather than greyed while a send is in flight: the
            // indicator takes its place, so the row keeps its shape.
            icon.source: messages.sending ? "" : "image://theme/icon-m-send"
            // A file on its own is a message; an empty field with nothing
            // attached is not. And nothing is sendable twice: copying a
            // large video into the core's blob directory takes long enough
            // for a second tap to land, and that sent the whole thing
            // again.
            enabled: !messages.sending
                     && (textField.text.length > 0
                         || page.attachmentPath.length > 0)
            onClicked: page.sendCurrentText()

            BusyIndicator {
                objectName: "sendBusy"
                anchors.centerIn: parent
                size: BusyIndicatorSize.Small
                running: messages.sending
            }
        }
    }

    function sendCurrentText() {
        // The model refuses a second send while one is outstanding and the
        // button is disabled meanwhile; this says so a third time because
        // EnterKey reaches here without going through the button.
        if (messages.sending) {
            return
        }
        // The bars are cleared from `onSent`, with the model's own copy:
        // clearing them here would drop the reply and the file the reader
        // chose on a send that never happened.
        if (page.attachmentPath.length > 0) {
            page.errorMessage = ""
            messages.send_file(textField.text, page.attachmentPath)
        } else if (textField.text.length > 0) {
            page.errorMessage = ""
            messages.send(textField.text)
        }
    }
}
