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
            // Now, not a second from now: the chat list would otherwise
            // show the message as a draft it is still holding, next to the
            // same message as the one just sent.
            page.storeDraft()
            page.replyBody = ""
            page.replyAuthor = ""
            page.attachmentPath = ""
            listView.jumpToNewest()
        }
        onArrived: listView.noteArrivals(count)
        // The chat's unsent text, once the core has answered with it.
        //
        // `draftApplied` is set only when something is actually put in the
        // field. Handing the chat over clears the draft and says so, and
        // treating that as the answer would mark the field filled before
        // the core had replied -- which is how this first went in and why
        // nothing came back.
        onDraft_changed: {
            if (!page.draftApplied && messages.draft.length > 0
                    && textField.text.length === 0) {
                textField.text = messages.draft
                page.draftApplied = true
            }
        }
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

    // A page pushed over this one takes the list's place in it with them:
    // it is torn down far enough to forget where it was, and comes back at
    // the top of whatever is loaded. Opening a picture full screen and
    // coming back is the way most readers meet that.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            listView.rememberPlace()
            // Written now rather than a second from now: leaving the chat
            // is exactly when the debounce below has not fired yet, and
            // that was the whole complaint.
            page.storeDraft()
        } else if (page.status === PageStatus.Active) {
            listView.restorePlace()
        }
    }

    /// Whether the chat's own draft has been put in the field yet.
    ///
    /// The answer comes back from the core a moment after the page opens,
    /// and a reader who started typing in that moment must not have it
    /// written over them.
    property bool draftApplied: false

    function storeDraft() {
        draftDebounce.stop()
        messages.save_draft(textField.text)
    }

    // Not on every keystroke: that is one call to the core per character.
    Timer {
        id: draftDebounce
        interval: 1000
        onTriggered: messages.save_draft(textField.text)
    }

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
                // Held rather than jumped to: the page is still arriving
                // and its rows still being measured, and one jump lands
                // the reader wherever the measuring has got to.
                listView.holdAt(row)
                foundFlash.restart()
            }
            // Once is enough, found or not: a later reload must not drag
            // the reader back off whatever they have scrolled to since.
            page.findMessageId = 0
        }
        // A fill takes at most a page at a time, and a screenful of a chat
        // with big rows in it can want more than that. Asking again when
        // one finishes is what covers the rest. It stops on its own: the
        // ask is dropped when there is nothing left to fill, so nothing
        // comes back to prompt another.
        onHydrating_changed: if (!messages.hydrating) listView.askForRows()
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
        // Null when the page could not be loaded, which is the case the
        // comment above is about; connecting to it would throw.
        if (picker) {
            picker.picked.connect(page.attach)
        }
    }

    // Outside the list rather than its `header`, so it stays put: a
    // header scrolls with the content, and in a long conversation the
    // name of whoever you are talking to disappears off the top. Not
    // Silica's PageHeader: the name is the other end's to choose, and
    // that header cannot be told to show it as plain text.
    ConversationHeader {
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
        // The model holds a row for every message and fills in the ones on
        // screen; this is what tells it which those are.
        onHydrateRequested: messages.hydrate(first, last)
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
        onOpenRequested: page.openAttachment(fileUrl, fileName, viewType)
        onDeleteRequested: messages.delete_message(messageId)
        onResendRequested: messages.resend_message(messageId)
        onForwardRequested: {
            // The picker reports back rather than acting, so the
            // message id is captured here where it is still valid.
            var travelling = messageId
            var picker = pageStack.push(
                Qt.resolvedUrl("ChatPickerPage.qml"),
                { accountId: page.accountId })
            if (picker) {
                picker.chatPicked.connect(function(chatId) {
                    messages.forward_to(travelling, chatId)
                })
            }
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
            // Kept in the core, so it is still here after the app has been
            // closed and reopened, and so the chat list can say which
            // chats are holding one.
            onTextChanged: draftDebounce.restart()
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
            // attached is not, and neither is one holding only spaces.
            // And nothing is sendable twice: copying a large video into
            // the core's blob directory takes long enough for a second
            // tap to land, and that sent the whole thing again.
            enabled: !messages.sending
                     && (textField.text.trim().length > 0
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

    // Which kinds Postivene shows itself, and which it hands on. Handing a
    // picture or a video to the system took the reader out of the app to
    // something that then failed to play it; everything else is still
    // somebody else's file to open, and a page here that could only say
    // "cannot show this" would be worse than the handover.
    function openAttachment(fileUrl, fileName, viewType) {
        if (viewType === "Image" || viewType === "Gif"
                || viewType === "Sticker") {
            pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName,
                viewType: viewType
            })
        } else if (viewType === "Video") {
            pageStack.push(Qt.resolvedUrl("VideoPage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName
            })
        } else {
            Qt.openUrlExternally(fileUrl)
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
        //
        // Trimmed: a message of nothing but whitespace is not a message,
        // and a trailing newline from the keyboard is not part of one.
        var text = textField.text.trim()
        if (page.attachmentPath.length > 0) {
            page.errorMessage = ""
            messages.send_file(text, page.attachmentPath)
        } else if (text.length > 0) {
            page.errorMessage = ""
            messages.send(text)
        }
    }
}
