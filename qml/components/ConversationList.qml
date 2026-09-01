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

    // Groups have to say who is speaking; one-to-one chats do not.
    property bool showSender: false
    property string placeholderText

    property bool stickToBottom: true

    /// The view has moved; these rows want filling in.
    ///
    /// The model holds a row for every message in the chat and fills in
    /// only the ones somebody is looking at, so this is what asks. Raised
    /// rather than acted on, like every other request here: the component
    /// knows nothing about the core.
    signal hydrateRequested(int first, int last)

    /// Ask for whatever is on screen to be filled in.
    ///
    /// `indexAt` lands on nothing between rows and over a day separator, so
    /// this walks the view rather than probing its two edges. Debounced by
    /// the timer below: a flick would otherwise ask once per frame.
    function askForRows() {
        var first = -1
        var last = -1
        for (var y = 1; y < root.height; y += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y)
            if (index >= 0) {
                if (first < 0) {
                    first = index
                }
                last = index
            }
        }
        if (first >= 0) {
            root.hydrateRequested(first, last)
        }
    }

    // Short: this is the wait between the reader arriving somewhere and the
    // rows there having anything in them, and the system's scroll-to-top
    // arrives in one step rather than over a flick's worth of frames. Long
    // enough still to coalesce a flick, which changes `contentY` every frame
    // and would otherwise walk the view for each of them.
    Timer {
        id: fillRows
        interval: 60
        onTriggered: root.askForRows()
    }

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
    /// The reader tapped an attachment. What opening it means -- a page
    /// here, or handing it to another app -- is the page's decision.
    signal openRequested(url fileUrl, string fileName, string viewType)

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
        onTriggered: {
            if (root.following) {
                root.positionViewAtEnd()
                root.reachedEnd = true
            }
        }
    }

    /// Whether the view has ever actually been put at the newest message.
    ///
    /// Only true after that, because until then a view sitting far from the
    /// end is a chat that has not opened yet rather than a reader who has
    /// gone somewhere.
    property bool reachedEnd: false

    // True between the start and end of a drag or flick. Tracked rather
    // than read off `moving`, which no test can set.
    property bool held: false

    // Not while the reader has hold of it. Rows are measured as they come
    // into view, so a drag upwards grows `contentHeight` on its own, and
    // following that hauls them straight back down again.
    readonly property bool following: root.stickToBottom && !root.held

    // Taking hold of the list is taking it over, so a held row lets go.
    onMovementStarted: {
        root.held = true
        root.releaseRow()
    }

    // Both of these, because a row arriving and that row being measured are
    // separate steps: the first moves `count`, the second `contentHeight`.
    onMessageCountChanged: if (root.following) toEnd.restart()
    onContentHeightChanged: {
        // A held row first: the content changing height is exactly what
        // moves the reader off it, so this is the moment to put them back.
        if (root.pendingRow >= 0) {
            root.putBack()
        } else if (root.following) {
            toEnd.restart()
        }
        // And whatever is on screen now wants filling in. Opening a chat
        // may never move contentY at all, so this is the ask that covers
        // the first screen.
        fillRows.restart()
    }

    onContentYChanged: {
        // While the page is away nobody is scrolling, so anything moving
        // the view is the list losing its place rather than the reader
        // choosing to.
        if (root.away && root.pendingRow >= 0) {
            root.putBack()
            return
        }
        // Scrolling through rows that are all still placeholders does not
        // change `contentHeight` at all -- they are the same height as each
        // other -- so without this the reader can walk into a screenful of
        // blanks and nothing ever asks for them.
        fillRows.restart()
        // Something has moved the view a long way from the newest message
        // without touching it: the system's own scroll-to-top, which is how
        // one gets to the beginning of a chat. Following would notice the
        // next row being measured and haul the reader straight back down,
        // which is being thrown to the newest message a moment after asking
        // for the oldest. A drag or a flick is excluded by `held`, a jump
        // this component made by `pendingRow`, and the chat still opening
        // by `reachedEnd`.
        if (root.reachedEnd && root.stickToBottom && !root.held
                && !root.restoring && root.pendingRow < 0 && !root.nearBottom) {
            root.stickToBottom = false
        }
    }

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

    /// Put the view on a row and keep it there while the rows settle.
    ///
    /// One `positionViewAtIndex` is enough only where every row is already
    /// its final height, which is nowhere real. Rows are measured as they
    /// are laid out, wrapped text at the device's own metrics is taller
    /// than an estimate, a picture's row changes height again when the
    /// picture decodes, and the header above the oldest row collapses to
    /// nothing the moment there is no more history to offer. Every one of
    /// those that happens above the reader moves them, and they all happen
    /// after the jump.
    ///
    /// So the row is held rather than jumped to: re-applied on each change
    /// to the content until the reader takes the view over or it stops
    /// moving under them. This is what a search result lands on and what
    /// the beginning of the chat lands on, and both were reported landing
    /// at the top of whatever had just loaded instead.
    function holdAt(index) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.pendingRow = index
        root.putBack()
        holdDeadline.restart()
    }

    /// Let go of the held row: the reader has taken over, or the view has
    /// stopped moving under them.
    function releaseRow() {
        holdDeadline.stop()
        root.away = false
        root.pendingRow = -1
    }

    /// The row a jump is holding the view on, or -1.
    property int pendingRow: -1
    /// True while `putBack` is running, so the view moving because this
    /// moved it does not read as one more reason to move it.
    property bool restoring: false

    function putBack() {
        if (root.restoring || root.pendingRow < 0) {
            return
        }
        root.restoring = true
        // The first row has nothing above it to be centred against, and
        // asking for its centre relies on the view clamping. Beginning is
        // what "the top" means.
        root.positionViewAtIndex(
            root.pendingRow,
            root.pendingRow === 0 ? ListView.Beginning : ListView.Center)
        root.restoring = false
    }

    // A held row is let go of when the reader takes the view over, and
    // otherwise not until this. There used to be a shorter timer as well,
    // restarted on each change, on the reasoning that the hold should last
    // exactly as long as the content was still moving -- but a device does
    // not move its content in one run. It lays the rows out, goes quiet
    // while a picture decodes, and moves them again; the gap was longer
    // than the timer, so the hold was gone by the time the reader was
    // carried off. Holding a view nobody is touching costs nothing.
    Timer {
        id: holdDeadline
        interval: 6000
        onTriggered: root.releaseRow()
    }

    /// The first row at or below `y` in the view.
    ///
    /// `indexAt` lands on nothing between rows or over a day separator, so
    /// a single probe answers -1 about half the time.
    function rowNear(y) {
        for (var offset = 0; y + offset < root.height; offset += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y + offset)
            if (index >= 0) {
                return index
            }
        }
        return -1
    }

    /// Where the reader is, before another page goes over this one.
    ///
    /// A conversation with a picture opened over it came back with its view
    /// at the top of the loaded messages: the list is torn down far enough
    /// to forget where it was, and what it forgets it replaces with the
    /// beginning. Remembered as a row rather than as a pixel offset, for
    /// the same reason a step back through the history is.
    property int rememberedRow: -1
    property bool rememberedFollowing: false
    /// True between the page going away and coming back, during which the
    /// row is held with no deadline: a reader can look at a picture for as
    /// long as they like.
    property bool away: false

    function rememberPlace() {
        root.rememberedFollowing = root.stickToBottom
        root.rememberedRow = root.rowNear(root.height / 2)
        if (root.rememberedRow >= 0 && !root.rememberedFollowing) {
            // Armed, not merely written down. Putting the view back when
            // the page returns is too late: the list is reset while the
            // page is away, and a frame showing the top of the chat is
            // painted before anything gets round to correcting it -- which
            // is the flash of the oldest messages, followed by being
            // yanked back, that this had left behind. Armed, the reset is
            // undone in the same turn it happens and no wrong frame is
            // ever drawn.
            root.away = true
            root.pendingRow = root.rememberedRow
            holdDeadline.stop()
        }
    }

    function restorePlace() {
        // The hold that was armed on the way out ends here, whatever
        // happens next: from now on the reader is looking at this page and
        // the deadline applies again.
        root.away = false
        if (root.rememberedFollowing) {
            root.jumpToNewest()
        } else if (root.rememberedRow >= 0) {
            // Held again rather than merely positioned: coming back is a
            // relayout like any other, and the rows settle after it.
            root.holdAt(root.rememberedRow)
        } else {
            root.releaseRow()
        }
        root.rememberedRow = -1
    }

    // How close to the end still counts as being at it. Exactly `atYEnd`
    // is the wrong test: rows are measured as they scroll into view, so
    // `contentHeight` is still growing at the moment a scroll stops, and
    // the comparison lands just short. That is why the button could not
    // be dismissed -- `jumpToNewest` set `stickToBottom`, the scroll it
    // started then ended, `onMovementEnded` read `atYEnd` as false and put
    // it straight back. A reader a line short of the bottom has, for every
    // purpose this drives, arrived.
    readonly property real bottomSlack: Theme.itemSizeLarge

    /// At or near the newest message -- including a chat too short to
    /// scroll at all, where there is no "end" to reach.
    readonly property bool nearBottom:
        root.contentHeight <= root.height
        || root.contentY + root.height
           >= root.contentHeight + root.originY - root.bottomSlack

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


    // The model counts days in the viewer's timezone, so grouping by that
    // number is enough to break the list into days.
    //
    // Declared without a `section.delegate`: the grouping is wanted, the
    // separate item is not. A section delegate is its own item, positioned
    // by the view above the row it heads and sized from whatever height it
    // reported when the view last measured it -- and a heading drawn over
    // the message beneath it was the report. Getting the height right at
    // creation was not enough, and the bookkeeping that decides where the
    // row goes is the view's rather than ours.
    //
    // So the heading is drawn *inside* the row instead, in `dayHeading`
    // below. `ListView.section` and `ListView.previousSection` come from
    // this property alone -- the view fills them in whether or not there is
    // a delegate to build -- so the view still says where a day starts, and
    // the heading is part of the row's own height. A row cannot be drawn
    // over itself.
    section.property: "day_number"

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
        // each other and the header. A row whose message has not been
        // fetched yet stands at one line, which is what gives the list a
        // length before any of it has been read. The day heading, when
        // this row carries one, is part of that height rather than
        // something the view has to find room for.
        contentHeight: dayHeading.height
                       + (model.loaded ? body.height : Theme.itemSizeExtraSmall)

        /// The date this row's day starts under, on the first row of each
        /// day and nowhere else.
        ///
        /// Inside the row rather than a `section.delegate`, so that where it
        /// sits is arithmetic here rather than the view's bookkeeping: see
        /// `section.property` above.
        Label {
            id: dayHeading
            objectName: "dayLabel"
            width: parent.width
            // The view fills these in from `section.property`. A row whose
            // day differs from the one before it is the first of its day;
            // the first row in the list has no previous section, which
            // reads as an empty string and so counts as a change.
            //
            // Day 0 is a row whose day is not known, which happens only if
            // the core answers the id list without day markers. Heading a
            // run of those with the epoch would be worse than heading them
            // with nothing.
            // Attached to the delegate root, not to this label: `ListView`
            // attaches to the item the view created. Outside a view both
            // read undefined, which is not "0" and does equal itself, so
            // this comes out false rather than erroring.
            visible: messageRow.ListView.section !== "0"
                     && messageRow.ListView.section
                        !== messageRow.ListView.previousSection
            height: visible ? implicitHeight + Theme.paddingMedium : 0
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            // No offset arithmetic: the section is a day number, so read
            // its calendar date in UTC and rebuild it as a local date with
            // the same parts. Subtracting an offset here would reintroduce
            // exactly the daylight-saving error the model now avoids.
            text: {
                var day = parseInt(messageRow.ListView.section, 10)
                if (isNaN(day)) {
                    return ""
                }
                var utc = new Date(day * 86400000)
                return Qt.formatDate(new Date(utc.getUTCFullYear(),
                                              utc.getUTCMonth(),
                                              utc.getUTCDate(), 12),
                                     Qt.DefaultLocaleLongDate)
            }
        }

        // Only what is on screen is built, so this is not the whole chat's
        // worth of delegates -- but a placeholder must not try to draw a
        // message it has not got.
        MessageDelegate {
            id: body
            visible: model.loaded
            y: dayHeading.height
            width: parent.width
            messageText: model.text
            isOutgoing: model.is_outgoing
            isInfo: model.is_info
            isForwarded: model.is_forwarded
            isFound: root.foundMessageId === model.message_id
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
            fileMime: model.file_mime
            fileBytes: model.file_bytes
            viewType: model.view_type
            imageWidth: model.image_width
            imageHeight: model.image_height
            vcardName: model.vcard_name
            vcardAddr: model.vcard_addr
            vcardColor: model.vcard_color
            onOpenRequested: root.openRequested(fileUrl, fileName, viewType)
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
