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
        onTriggered: if (root.stickToBottom) root.positionViewAtEnd()
    }

    // Both of these, because a row arriving and that row being measured are
    // separate steps: the first moves `count`, the second `contentHeight`.
    onCountChanged: if (root.stickToBottom) toEnd.restart()
    onContentHeightChanged: if (root.stickToBottom) toEnd.restart()
    // Where the reader left off, once they stop moving. `atYEnd` is the
    // view's own answer; the arithmetic version has to know about `originY`
    // and gets it wrong.
    onMovementEnded: root.stickToBottom = root.atYEnd

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
        objectName: "messageRow"
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
