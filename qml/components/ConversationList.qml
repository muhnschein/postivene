import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The messages of one conversation. Its own component so the scrolling can
 * be driven in a test -- ConversationPage cannot be loaded headlessly.
 *
 * Opens on the newest message and follows arrivals, but only while the
 * reader is already down there: scrolling away from history someone is
 * reading is worse than missing an arrival.
 */
SilicaListView {
    id: root

    property string title
    // Seconds east of UTC, for turning a day number back into a date.
    // Groups have to say who is speaking; one-to-one chats do not.
    property bool showSender: false
    property string placeholderText

    property bool stickToBottom: true
    // How many rows the model holds. Bound by the page rather than read off
    // the view: `count` there only changes when the view has laid out, and
    // an arrival has to be noticed whether or not it is on screen yet.
    property int messageCount: 0
    // How many have arrived since the reader scrolled away. Counted from
    // what the model says arrived, not by differencing `messageCount`: a
    // deletion moves that too, and a removal landing with an arrival in one
    // reload does not move it at all.
    property int missedCount: 0

    /// Messages from other people have just been added.
    function noteArrivals(count) {
        if (!root.following) {
            root.missedCount += count
        }
    }

    // Raised rather than acted on: the component knows nothing about the
    // core, which is what makes it loadable on its own.
    signal replyRequested(int messageId, string body, string author)
    signal copyRequested(string body)
    signal deleteRequested(int messageId)
    signal resendRequested(int messageId)
    signal forwardRequested(int messageId)

    /// Back to the newest message, and following again.
    function jumpToNewest() {
        // The button sits over the list rather than inside it, so a tap
        // does not stop an inertial flick the way touching the list would.
        // Left running, `held` stays true, `following` stays false, and the
        // scroll below never happens -- while the state this sets has
        // already told the page the reader is at the newest message.
        root.cancelFlick()
        root.held = false
        root.stickToBottom = true
        root.missedCount = 0
        toEnd.restart()
    }

    // A list draws its delegates outside its own box unless told not to,
    // and what sits below this one is translucent.
    clip: true

    // Never straight away: a model handed in at construction has already
    // filled, and rows still have to be measured, so where the end is is
    // not known until the pass after whatever prompted this.
    Timer {
        id: toEnd
        interval: 0
        // Checked again here: the reader can scroll away between the
        // arrival that started this and the pass it fires on.
        onTriggered: if (root.following) root.positionViewAtEnd()
    }

    // True between the start and end of a drag or flick. Tracked rather
    // than read off `moving`, which no test can set.
    property bool held: false

    // Not while the reader has hold of it. Rows are measured as they come
    // into view, so a drag upwards grows `contentHeight` on its own, and
    // following that hauls them straight back down again.
    readonly property bool following: root.stickToBottom && !root.held

    onMovementStarted: root.held = true

    // Both of these, because a row arriving and that row being measured are
    // separate steps: the first moves `count`, the second `contentHeight`.
    onMessageCountChanged: if (root.following) toEnd.restart()
    onContentHeightChanged: if (root.following) toEnd.restart()
    // How close to the end still counts as being at it. Exactly `atYEnd`
    // is the wrong test: rows are measured as they scroll into view, so
    // `contentHeight` is still growing at the moment a scroll stops, and
    // the comparison lands just short. That is why the button could not
    // be dismissed -- `jumpToNewest` set `stickToBottom`, the scroll it
    // started then ended, and this handler read `atYEnd` as false and put
    // it straight back. A reader a line short of the bottom has, for every
    // purpose this drives, arrived.
    /// The message a search sent the reader here for, flashed once so it
    /// can be picked out of the wall of text around it. 0 for none.
    property int foundMessageId: 0

    /// Put a row in the middle of the view and stay there.
    ///
    /// Opening a chat at its newest message when the reader asked for one
    /// from last March is the difference between finding something and
    /// being told roughly where it is. `stickToBottom` goes off first:
    /// otherwise the next arrival drags the view back to the bottom and
    /// away from what they came to read.
    function jumpToRow(index) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.positionViewAtIndex(index, ListView.Center)
    }

    readonly property real bottomSlack: Theme.itemSizeLarge

    /// At or near the newest message -- including a chat too short to
    /// scroll at all, where there is no "end" to reach.
    readonly property bool nearBottom:
        root.contentHeight <= root.height
        || root.contentY + root.height >= root.contentHeight + root.originY - root.bottomSlack

    // Where the reader left off, once they stop moving.
    onMovementEnded: {
        root.held = false
        root.stickToBottom = root.nearBottom
        if (root.nearBottom) {
            root.missedCount = 0
        }
    }

    // Arriving at the newest message, by scrolling or by the button, is
    // what counts as having read what is there.
    onStickToBottomChanged: if (root.stickToBottom) root.arrivedAtNewest()
    signal arrivedAtNewest()

    header: PageHeader {
        title: root.title
    }

    // The model counts days in the viewer's timezone, so grouping by that
    // number is enough to break the list into days.
    section.property: "day_number"
    section.delegate: Label {
        objectName: "dayLabel"
        width: root.width
        horizontalAlignment: Text.AlignHCenter
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        // No offset arithmetic: the section is a day number, so read its
        // calendar date in UTC and rebuild it as a local date with the
        // same parts. Subtracting an offset here would reintroduce
        // exactly the daylight-saving error the model now avoids.
        text: {
            var utc = new Date(parseInt(section, 10) * 86400000)
            return Qt.formatDate(new Date(utc.getUTCFullYear(),
                                          utc.getUTCMonth(),
                                          utc.getUTCDate(), 12),
                                 Qt.DefaultLocaleLongDate)
        }
    }

    delegate: ListItem {
        id: messageRow
        objectName: "messageRow"

        menu: ContextMenu {
            MenuItem {
                objectName: "replyItem"
                // A core notice is nobody's message to answer.
                visible: !model.is_info
                text: qsTr("Reply")
                onClicked: root.replyRequested(model.message_id, model.text,
                                               model.sender_name)
            }
            MenuItem {
                objectName: "copyItem"
                // An image or a voice message with no caption has no text:
                // copying one emptied the clipboard and said it had worked.
                visible: model.text.length > 0
                text: qsTr("Copy")
                onClicked: root.copyRequested(model.text)
            }
            MenuItem {
                objectName: "forwardItem"
                // A core notice is not the reader's to pass on.
                visible: !model.is_info
                text: qsTr("Forward")
                // Taken now rather than in the callback: picking a chat
                // takes a page push, and this row may be gone by the time
                // the answer comes back -- the same reason Delete hoists
                // its id.
                onClicked: root.forwardRequested(model.message_id)
            }
            MenuItem {
                objectName: "resendItem"
                // DC_STATE_OUT_FAILED: the only state worth retrying.
                visible: model.state === 24
                text: qsTr("Send again")
                onClicked: root.resendRequested(model.message_id)
            }
            MenuItem {
                objectName: "deleteItem"
                text: qsTr("Delete")
                // Taken now rather than read in the callback: anything that
                // reloads the model destroys this row, Silica runs the
                // action as it goes, and `model` is gone by then.
                onClicked: {
                    var doomed = model.message_id
                    messageRow.remorseAction(qsTr("Deleting"), function() {
                        root.deleteRequested(doomed)
                    })
                }
            }
        }
        // Sized by its content, not fixed: a device message runs to a
        // dozen wrapped lines, and a fixed row height makes them overlap
        // each other and the header.
        contentHeight: body.height

        MessageDelegate {
            id: body
            width: parent.width
            messageText: model.text
            isOutgoing: model.is_outgoing
            isInfo: model.is_info
            isForwarded: model.is_forwarded
            showPadlock: model.show_padlock
            deliveryState: model.state
            sentAt: model.timestamp
            senderName: model.sender_name
            senderColor: model.sender_color
            showSender: root.showSender
            quoteText: model.quote_text
            quoteAuthor: model.quote_author
            filePath: model.file_path
            fileName: model.file_name
            viewType: model.view_type
            imageWidth: model.image_width
            imageHeight: model.image_height
        }
    }

    /// Whether the chat's messages have actually been fetched yet.
    ///
    /// Not the same as having none. Without it, every open flashed "no
    /// messages yet" while the history was still on its way.
    property bool loaded: true

    ViewPlaceholder {
        objectName: "emptyPlaceholder"
        enabled: root.loaded && root.count === 0
        text: root.placeholderText
    }
}
