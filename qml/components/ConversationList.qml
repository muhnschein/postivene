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
    property int utcOffset: 0
    // Groups have to say who is speaking; one-to-one chats do not.
    property bool showSender: false
    property string placeholderText

    property bool stickToBottom: true
    // How many rows the model holds. Bound by the page rather than read off
    // the view: `count` there only changes when the view has laid out, and
    // an arrival has to be noticed whether or not it is on screen yet.
    property int messageCount: 0
    // How many have arrived since the reader scrolled away.
    property int missedCount: 0
    property int lastCount: 0

    // Raised rather than acted on: the component knows nothing about the
    // core, which is what makes it loadable on its own.
    signal replyRequested(int messageId, string body, string author)
    signal copyRequested(string body)
    signal deleteRequested(int messageId)
    signal resendRequested(int messageId)

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
    onMessageCountChanged: {
        if (root.following) {
            toEnd.restart()
        } else if (root.messageCount > root.lastCount) {
            // Only arrivals count: deleting a message also moves this.
            root.missedCount += root.messageCount - root.lastCount
        }
        root.lastCount = root.messageCount
    }
    onContentHeightChanged: if (root.following) toEnd.restart()
    // Where the reader left off, once they stop moving. `atYEnd` is the
    // view's own answer; the arithmetic version has to know about `originY`
    // and gets it wrong.
    onMovementEnded: {
        root.held = false
        root.stickToBottom = root.atYEnd
        if (root.atYEnd) {
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
        // Local midnight of that day, back from the day number.
        text: Qt.formatDate(new Date((parseInt(section, 10) * 86400
                                      - root.utcOffset) * 1000),
                            Qt.DefaultLocaleLongDate)
    }

    delegate: ListItem {
        id: messageRow
        objectName: "messageRow"

        menu: ContextMenu {
            MenuItem {
                objectName: "replyItem"
                text: qsTr("Reply")
                onClicked: root.replyRequested(model.message_id, model.text,
                                               model.sender_name)
            }
            MenuItem {
                objectName: "copyItem"
                text: qsTr("Copy")
                onClicked: root.copyRequested(model.text)
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

    ViewPlaceholder {
        enabled: root.count === 0
        text: root.placeholderText
    }
}
