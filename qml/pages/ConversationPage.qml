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

    SilicaListView {
        id: listView
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        // A list draws its delegates outside its own box unless told not
        // to, and the message field below is translucent: without this the
        // last messages show through it.
        clip: true
        model: messages.rows

        header: PageHeader {
            title: page.chatName
        }

        // The model counts days in the viewer's timezone, so grouping by
        // that number is enough to break the list into days.
        section.property: "day_number"
        section.delegate: Label {
            objectName: "dayLabel"
            width: listView.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            // Local midnight of that day, back from the day number.
            text: Qt.formatDate(new Date((parseInt(section, 10) * 86400
                                          - page.utcOffset) * 1000),
                                Qt.DefaultLocaleLongDate)
        }

        delegate: ListItem {
            objectName: "messageRow"
            // Sized by its content, not fixed: a device message runs to a
            // dozen wrapped lines, and a fixed row height makes them
            // overlap each other and the header.
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
                showSender: messages.is_group
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
            enabled: listView.count === 0
            text: qsTr("No messages yet")
        }
    }

    // Between the list and the field rather than over the list: it is
    // translucent, and the messages behind it showed through.
    ErrorBanner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        // A dead core outranks whatever failed before it, and a page opened
        // after it died never saw the transition -- so read the status
        // rather than waiting for it to change.
        text: core.status === "stopped" ? page.coreStoppedMessage : page.errorMessage
        // That one does not fix itself, so it stays put.
        timeout: core.status === "stopped" ? 0 : 8
        onDismissed: page.errorMessage = ""
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
            messages.send(textField.text)
        }
    }
}
