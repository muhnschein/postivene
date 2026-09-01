import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format

/*
 * One message. Its own component so it can be loaded and measured on its
 * own: ConversationPage cannot, because Silica's EnterKey attached property
 * has no stub.
 *
 * Every string here comes from whoever sent the message, so each one
 * that shows it is pinned to PlainText: the default detects markup and
 * switches to rich text, which would let a message body fetch a remote
 * image the moment its row is drawn.
 *
 * Laid out by bindings rather than by a Column: a positioner sizes itself in
 * a polish pass, which never runs headlessly, so a row built from one cannot
 * be measured in a test.
 */
Item {
    id: root

    /// The reader asked to open the attachment. The URL rather than the
    /// path: the encoding it needs is already done once, in the preview.
    signal openRequested(url fileUrl, string fileName, string viewType)

    property string messageText: ""
    property bool isOutgoing: false
    property bool isInfo: false
    property bool isForwarded: false
    /// This is the message a search sent the reader here for.
    property bool isFound: false
    property bool showPadlock: true
    // DC_STATE_*: 20 pending, 24 failed, 26 delivered, 28 read.
    property int deliveryState: 0
    // Unix seconds.
    property double sentAt: 0
    property string senderName: ""
    property string senderColor: ""
    // Only groups need to say who is speaking.
    property bool showSender: false
    property string quoteText: ""
    property string quoteAuthor: ""
    property string filePath: ""
    property string fileName: ""
    property string fileMime: ""
    // A real, not an int: QML has no 64-bit integer to hold a file size in.
    property real fileBytes: 0
    // Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc,
    // Vcard. What the attachment is drawn as; AttachmentPreview decides.
    property string viewType: "Text"
    property int imageWidth: 0
    property int imageHeight: 0
    // A shared contact, parsed by the core.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""

    property bool hasFile: filePath.length > 0
    // A sticker is a picture with no bubble behind it, which is the whole
    // of what makes it one.
    readonly property bool isSticker: root.viewType === "Sticker" && root.hasFile

    // A bubble is as wide as its content, up to most of the screen. The
    // widths come off unconstrained copies of the text: measuring the real
    // labels, whose width comes back from the bubble, is a binding loop.
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    property real contentWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 attachment.wantsFullWidth && root.hasFile ? root.maxWidth : 0,
                 Theme.itemSizeSmall))

    height: (root.isInfo ? infoLabel.height : bubble.height) + 2 * Theme.paddingSmall

    // Where the next part goes: right below the last one that is there,
    // with a gap only when both are.
    function below(previous, mine) {
        return previous.y + previous.height
               + (mine && previous.height > 0 ? Theme.paddingSmall : 0)
    }

    Text {
        id: textMetric
        visible: false
        font: messageLabel.font
        textFormat: Text.PlainText
        text: root.messageText
    }

    Text {
        id: attachmentMetric
        visible: false
        font.pixelSize: Theme.fontSizeMedium
        textFormat: Text.PlainText
        // Asked of the preview rather than read off its label: what the
        // fallback row says is the preview's business, and the bubble only
        // needs to know how wide it comes out.
        text: attachment.genericText
    }

    // A core notice, not something anyone typed: centred and unadorned.
    Label {
        id: infoLabel
        objectName: "infoLabel"
        visible: root.isInfo
        height: visible ? implicitHeight : 0
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.horizontalPageMargin
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        text: root.messageText
    }

    Rectangle {
        id: bubble
        objectName: "bubble"
        visible: !root.isInfo
        x: root.isOutgoing
           ? root.width - width - Theme.horizontalPageMargin
           : Theme.horizontalPageMargin
        width: root.contentWidth + 2 * Theme.paddingMedium
        height: visible ? footerLabel.y + footerLabel.height + Theme.paddingMedium : 0
        radius: Theme.paddingMedium
        // A found message is lit rather than outlined: a border would
        // change the bubble's size, and every row below it would move.
        color: root.isFound
               ? Theme.rgba(Theme.highlightColor, 0.5)
               // A sticker is meant to sit on the conversation rather than
               // in a bubble; the rest of the row still lays out the same.
               : root.isSticker ? "transparent"
               : root.isOutgoing ? Theme.rgba(Theme.highlightBackgroundColor, 0.25)
                                 : Theme.rgba(Theme.secondaryColor, 0.12)
        Behavior on color { ColorAnimation { duration: 400 } }

        Label {
            id: senderLabel
            objectName: "senderLabel"
            visible: root.showSender && !root.isOutgoing && root.senderName.length > 0
            height: visible ? implicitHeight : 0
            x: Theme.paddingMedium
            y: Theme.paddingMedium
            width: root.contentWidth
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: root.senderColor.length > 0 ? root.senderColor : Theme.highlightColor
            textFormat: Text.PlainText
            text: root.senderName
        }

        // Marked the way the reference client marks it, and above the
        // quote: it describes the whole message, not the quoted part.
        // Not remote-supplied text, so it needs no PlainText pinning --
        // but it must not be folded into messageText, which is.
        Label {
            id: forwardedLabel
            objectName: "forwardedLabel"
            visible: root.isForwarded
            height: visible ? implicitHeight : 0
            x: Theme.paddingMedium
            y: root.below(senderLabel, visible)
            width: root.contentWidth
            font.pixelSize: Theme.fontSizeExtraSmall
            font.italic: true
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: qsTr("Forwarded")
        }

        // The quoted message, marked off by a bar down its left.
        Item {
            id: quoteRow
            objectName: "quoteRow"
            visible: root.quoteText.length > 0
            x: Theme.paddingMedium
            y: root.below(forwardedLabel, visible)
            width: root.contentWidth
            height: visible ? quoteLabel.y + quoteLabel.height : 0

            Rectangle {
                width: 2
                height: parent.height
                color: Theme.highlightColor
            }

            Label {
                id: quoteAuthorLabel
                x: 2 + Theme.paddingSmall
                width: parent.width - 2 - Theme.paddingSmall
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: root.quoteAuthor
            }

            Label {
                id: quoteLabel
                objectName: "quoteLabel"
                x: quoteAuthorLabel.x
                y: quoteAuthorLabel.height
                width: quoteAuthorLabel.width
                maximumLineCount: 2
                wrapMode: Text.Wrap
                truncationMode: TruncationMode.Elide
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                text: root.quoteText
            }
        }

        // Whatever kind of attachment this is, drawn by the one component
        // that knows the difference. Reports rather than acts, so opening
        // stays the page's decision.
        AttachmentPreview {
            id: attachment
            objectName: "attachment"
            x: Theme.paddingMedium
            y: root.below(quoteRow, height > 0)
            contentWidth: root.contentWidth
            filePath: root.filePath
            fileName: root.fileName
            fileMime: root.fileMime
            fileBytes: root.fileBytes
            viewType: root.viewType
            imageWidth: root.imageWidth
            imageHeight: root.imageHeight
            vcardName: root.vcardName
            vcardAddr: root.vcardAddr
            vcardColor: root.vcardColor
            // Passed up rather than acted on, which is what the comment
            // above always said and what this row now actually does: what
            // opening an attachment means is a page's decision, and a
            // delegate cannot push one.
            onOpenRequested: root.openRequested(attachment.fileUrl,
                                                root.fileName, root.viewType)
        }

        Label {
            id: messageLabel
            objectName: "messageLabel"
            visible: root.messageText.length > 0
            height: visible ? implicitHeight : 0
            x: Theme.paddingMedium
            y: root.below(attachment, visible)
            width: root.contentWidth
            wrapMode: Text.Wrap
            color: Theme.primaryColor
            textFormat: Text.PlainText
            text: root.messageText
        }

        // Time, and for our own messages how far it got. A mail icon marks
        // anything that was not encrypted and signed.
        Label {
            id: footerLabel
            objectName: "footerLabel"
            x: Theme.paddingMedium
            y: root.below(messageLabel, true)
            width: root.contentWidth
            horizontalAlignment: Text.AlignRight
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: (root.showPadlock ? "" : "✉ ")
                  + Qt.formatTime(new Date(root.sentAt * 1000), "hh:mm")
                  + (root.isOutgoing ? " " + Format.stateMark(root.deliveryState) : "")
        }
    }
}
