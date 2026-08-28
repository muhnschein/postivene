import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One message. Its own component so it can be loaded and measured on its
 * own: ConversationPage cannot, because Silica's EnterKey attached property
 * has no stub.
 *
 * Laid out by bindings rather than by a Column: a positioner sizes itself in
 * a polish pass, which never runs headlessly, so a row built from one cannot
 * be measured in a test.
 */
Item {
    id: root

    property string messageText: ""
    property bool isOutgoing: false
    property bool isInfo: false
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
    // Text, Image, Gif, Sticker, Audio, Voice, Video, File, Webxdc, Vcard.
    property string viewType: "Text"
    property int imageWidth: 0
    property int imageHeight: 0

    property bool isPicture: viewType === "Image" || viewType === "Gif"
                             || viewType === "Sticker"
    property bool hasFile: filePath.length > 0

    // A bubble is as wide as its content, up to most of the screen. The
    // widths come off unconstrained copies of the text: measuring the real
    // labels, whose width comes back from the bubble, is a binding loop.
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    property real contentWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 root.isPicture && root.hasFile ? root.maxWidth : 0,
                 Theme.itemSizeSmall))

    height: (root.isInfo ? infoLabel.height : bubble.height) + 2 * Theme.paddingSmall

    // Where the next part goes: right below the last one that is there,
    // with a gap only when both are.
    function below(previous, mine) {
        return previous.y + previous.height
               + (mine && previous.height > 0 ? Theme.paddingSmall : 0)
    }

    function stateMark(state) {
        if (state === 28) return "✓✓"
        if (state === 26) return "✓"
        if (state === 24) return "✗"
        if (state === 20) return "…"
        return ""
    }

    Text {
        id: textMetric
        visible: false
        font: messageLabel.font
        text: root.messageText
    }

    Text {
        id: attachmentMetric
        visible: false
        font: attachmentLabel.font
        text: attachmentLabel.text
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
        color: root.isOutgoing ? Theme.rgba(Theme.highlightBackgroundColor, 0.25)
                               : Theme.rgba(Theme.secondaryColor, 0.12)

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
            text: root.senderName
        }

        // The quoted message, marked off by a bar down its left.
        Item {
            id: quoteRow
            objectName: "quoteRow"
            visible: root.quoteText.length > 0
            x: Theme.paddingMedium
            y: root.below(senderLabel, visible)
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
                text: root.quoteText
            }
        }

        Image {
            id: attachmentImage
            objectName: "attachmentImage"
            visible: root.isPicture && root.hasFile
            x: Theme.paddingMedium
            y: root.below(quoteRow, visible)
            width: visible ? root.contentWidth : 0
            height: visible && root.imageWidth > 0
                    ? width * root.imageHeight / root.imageWidth
                    : 0
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            source: root.hasFile ? "file://" + root.filePath : ""
        }

        // Anything else with a file: name it and let the system open it.
        Label {
            id: attachmentLabel
            objectName: "attachmentLabel"
            visible: root.hasFile && !root.isPicture
            height: visible ? implicitHeight : 0
            x: Theme.paddingMedium
            y: root.below(attachmentImage, visible)
            width: root.contentWidth
            truncationMode: TruncationMode.Fade
            color: Theme.highlightColor
            text: "📎 " + (root.fileName.length > 0 ? root.fileName : root.filePath)

            MouseArea {
                anchors.fill: parent
                onClicked: Qt.openUrlExternally("file://" + root.filePath)
            }
        }

        Label {
            id: messageLabel
            objectName: "messageLabel"
            visible: root.messageText.length > 0
            height: visible ? implicitHeight : 0
            x: Theme.paddingMedium
            y: root.below(attachmentLabel, visible)
            width: root.contentWidth
            wrapMode: Text.Wrap
            color: Theme.primaryColor
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
                  + (root.isOutgoing ? " " + root.stateMark(root.deliveryState) : "")
        }
    }
}
