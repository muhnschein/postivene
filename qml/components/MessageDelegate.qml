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
 * image the moment its row is drawn. The one exception is deliberate: with
 * Markdown drawn, the body is the shim's own rendering of it
 * (markdown.rs), in which every character of the message is escaped and
 * the only tags are the ones the shim wrote -- and it is shown as
 * StyledText, never RichText, so nothing in it can load anything.
 *
 * Laid out by bindings rather than by a Column: a positioner sizes itself in
 * a polish pass, which never runs headlessly, so a row built from one cannot
 * be measured in a test.
 */
Item {
    id: root

    /// The reader asked to open the attachment. The URL rather than the
    /// path: the encoding it needs is already done once, in the preview.
    /// `previewWidth` is how wide the picture was drawn here, so a page
    /// opening it full screen can start from the same decode.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// The reader asked for the rest of a message the core holds only
    /// the header of.
    signal downloadRequested()
    /// The reader tapped a reaction chip: put that emoji on the message,
    /// or take it off again when it is already theirs. The model decides
    /// which; the row only says what was tapped.
    signal reactionRequested(string emoji)
    /// A long press landed on a control that takes presses for itself --
    /// a chip, the download offer, the play button -- and the row's menu
    /// is what a long press means anywhere on a message.
    signal menuRequested()

    /// What a tap on the message does. The row's own tap, not one of the
    /// attachment's: a picture with a tap area of its own took the press
    /// away from the row, so a long press on it never reached the menu,
    /// and a tap just off it did nothing. The whole message is one
    /// surface now -- a tap opens what there is to open, a long press
    /// opens the menu -- and the two cannot fight over a pixel.
    function tapped() {
        if (attachment.openable) {
            root.openRequested(attachment.fileUrl, root.fileName, root.viewType,
                               attachment.contentWidth)
        } else if (root.canDownload) {
            root.downloadRequested()
        }
    }

    property string messageText: ""
    /// The reactions on this message, as the shim hands them over: a JSON
    /// array of {emoji, count, self}, most frequent first. Empty for none.
    property string reactions: ""
    /// The same, parsed once per change rather than once per chip.
    readonly property var reactionList: root.reactions.length > 0
                                        ? JSON.parse(root.reactions) : []
    /// The same text rendered as StyledText by the shim, and with its
    /// Markdown taken out. Empty for a row that has neither, which is
    /// shown as written.
    property string styledText: ""
    property string plainText: ""
    /// 0 draws Markdown, 1 takes its markers out, 2 shows it as written.
    property int markdownMode: 2
    /// `downloadState` upstream: Done, Available, InProgress, Failure or
    /// Undecipherable. Anything but Done and empty is a message the core
    /// has only the header of.
    property string downloadState: ""
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
    /// A message the reader has not seen before; see AttachmentPreview.
    property bool isNew: false
    // A shared contact, parsed by the core.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""

    property bool hasFile: filePath.length > 0
    // A sticker is a picture with no bubble behind it, which is the whole
    // of what makes it one.
    readonly property bool isSticker: root.viewType === "Sticker" && root.hasFile

    /// Whether the body is drawn from the shim's rendering. Only with a
    /// rendering to draw: the raw text as StyledText would be the very
    /// thing the plain-text pinning is for.
    readonly property bool drawsStyled: root.markdownMode === 0
                                        && root.styledText.length > 0
    /// The body as the setting wants it shown.
    readonly property string shownText: root.drawsStyled
                                        ? root.styledText
                                        : root.markdownMode === 1 && root.plainText.length > 0
                                          ? root.plainText
                                          : root.messageText
    /// A message the core has only the header of, or is fetching, or
    /// could not fetch: something to say, and mostly something to tap.
    readonly property bool heldBack: root.downloadState.length > 0
                                     && root.downloadState !== "Done"
    /// The two states the rest of a message can be asked for in.
    readonly property bool canDownload: root.downloadState === "Available"
                                        || root.downloadState === "Failure"

    // A bubble is as wide as its content, up to most of the screen. The
    // widths come off unconstrained copies of the text: measuring the real
    // labels, whose width comes back from the bubble, is a binding loop.
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    property real contentWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 reactionRow.wantedWidth,
                 attachment.wantsFullWidth && root.hasFile ? root.maxWidth : 0,
                 Theme.itemSizeSmall))

    // The chips hang below the bubble, and what hangs is the row's to
    // make room for: without it they draw over the next message.
    height: (root.isInfo ? infoLabel.height : bubble.height)
            + (reactionRow.visible ? reactionRow.height - root.chipOverlap : 0)
            + 2 * Theme.paddingSmall

    /// How far the chips reach up over the bubble's bottom edge: enough
    /// to read as hung on it, and no more, since the footer's time sits
    /// in that corner and the chips went over it at half their height.
    readonly property real chipOverlap: Theme.paddingMedium - Theme.paddingSmall / 2

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
        // Measured as it will be drawn: bold is wider than plain.
        textFormat: root.drawsStyled ? Text.StyledText : Text.PlainText
        text: root.shownText
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

    // The chips' text end to end, for how wide the strip wants to be and
    // how tall one line of it is. The chips themselves are measured one
    // by one below; this is the sum the bubble's width is decided from
    // before any of them exists.
    Text {
        id: reactionMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: {
            var parts = []
            for (var i = 0; i < root.reactionList.length; i++) {
                parts.push(root.chipText(root.reactionList[i]))
            }
            return parts.join(" ")
        }
    }

    /// What a chip says: the emoji, and how many when it is more than one
    /// person. "👍" reads as one; "👍 1" reads as a score.
    function chipText(reaction) {
        return reaction.count > 1 ? reaction.emoji + " " + reaction.count
                                  : reaction.emoji
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
            wrapMode: Text.Wrap
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
            isNew: root.isNew
            vcardName: root.vcardName
            vcardAddr: root.vcardAddr
            vcardColor: root.vcardColor
            // A long press on one of its own controls is the row's menu.
            onMenuRequested: root.menuRequested()
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
            linkColor: Theme.highlightColor
            // Plain, unless the shim rendered it: see the note at the top.
            textFormat: root.drawsStyled ? Text.StyledText : Text.PlainText
            text: root.shownText
            // A link is followed on a tap and on nothing else.
            onLinkActivated: Qt.openUrlExternally(link)
        }

        // A message the download limit held back: the core has its
        // header and this is where the rest is asked for.
        Label {
            id: downloadLabel
            objectName: "downloadButton"
            visible: root.heldBack
            height: visible ? implicitHeight + Theme.paddingSmall : 0
            x: Theme.paddingMedium
            y: root.below(messageLabel, visible)
            width: root.contentWidth
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
            // Translated literals, chosen by the core's state.
            textFormat: Text.PlainText
            text: root.downloadState === "InProgress"
                  ? qsTr("Downloading…")
                  : root.downloadState === "Failure"
                    ? qsTr("⬇ Download failed, tap to try again")
                    : root.downloadState === "Undecipherable"
                      ? qsTr("Cannot be decrypted")
                      //: Fetches a message the auto-download limit held back.
                      : qsTr("⬇ Download")

            MouseArea {
                anchors.fill: parent
                enabled: root.canDownload
                onClicked: root.downloadRequested()
                onPressAndHold: root.menuRequested()
            }
        }

        // Time, and for our own messages how far it got. A mail icon marks
        // anything that was not encrypted and signed.
        Label {
            id: footerLabel
            objectName: "footerLabel"
            x: Theme.paddingMedium
            y: root.below(downloadLabel, true)
            width: root.contentWidth
            horizontalAlignment: Text.AlignRight
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: (root.showPadlock ? "" : "✉ ")
                  + Qt.formatTime(new Date(root.sentAt * 1000), "hh:mm")
                  + (root.isOutgoing ? " " + Format.stateMark(root.deliveryState) : "")
        }
    }

    // Who reacted with what, one chip per emoji, ours lit. Hung off the
    // bubble's bottom corner rather than laid out inside it -- the
    // inside corner, towards the middle of the screen, the way the
    // reference clients hang theirs: just over the bubble's edge and
    // mostly below it, so a reaction reads as something put on the
    // message rather than a line of it, and the time in that corner
    // stays readable. Laid out by hand rather than in a Row,
    // for the reason at the top of the file: each chip sits after the
    // ones before it, and reads their widths so a chip that grows
    // pushes the rest along. One line, clipped: a message with more
    // distinct reactions than fit is rare enough that the last of them
    // can go unseen.
    Item {
        id: reactionRow
        objectName: "reactionRow"
        visible: !root.isInfo && root.reactionList.length > 0
        // Inside the bubble's own padding, at the corner nearest the
        // middle of the screen.
        x: root.isOutgoing ? bubble.x + Theme.paddingMedium
                           : bubble.x + bubble.width - width - Theme.paddingMedium
        y: bubble.height - root.chipOverlap
        // Never wider than the bubble, which sizes itself to fit the
        // chips where it can.
        width: Math.min(wantedWidth, root.contentWidth)
        // A line of the chip font plus the chip's own padding.
        height: visible ? reactionMetric.height + 2 * Theme.paddingSmall : 0
        clip: true

        /// The room the chips take in a row: their text, each one's
        /// padding, and the gaps between them.
        readonly property real wantedWidth:
            root.reactionList.length === 0 ? 0
            : reactionMetric.implicitWidth
              + root.reactionList.length * 2 * Theme.paddingMedium
              + (root.reactionList.length - 1) * Theme.paddingSmall

        Repeater {
            id: reactionRepeater
            objectName: "reactionRepeater"
            model: root.reactionList

            Rectangle {
                objectName: "reactionChip"
                /// Whether this is the reader's own reaction.
                readonly property bool mine: modelData.self === true
                /// The emoji, for the tap to name.
                readonly property string emoji: modelData.emoji
                // After every chip before it. Reading their widths is
                // what makes this follow them: the repeater builds
                // chips in order, so each one it asks for is there.
                x: {
                    var at = 0
                    for (var i = 0; i < index; i++) {
                        var earlier = reactionRepeater.itemAt(i)
                        if (earlier) {
                            at += earlier.width + Theme.paddingSmall
                        }
                    }
                    return at
                }
                width: chipLabel.implicitWidth + 2 * Theme.paddingMedium
                height: reactionRow.height
                radius: height / 2
                // Nearly solid: a chip straddles the bubble's edge, and
                // a translucent one would change colour halfway down.
                color: mine ? Theme.rgba(Theme.highlightBackgroundColor, 0.9)
                            : Theme.rgba(Theme.highlightDimmerColor, 0.9)

                Label {
                    id: chipLabel
                    objectName: "chipLabel"
                    anchors.centerIn: parent
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.primaryColor
                    // The emoji is whatever the other end sent, and
                    // the core does not check that it is one.
                    textFormat: Text.PlainText
                    text: root.chipText(modelData)
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: root.reactionRequested(emoji)
                    onPressAndHold: root.menuRequested()
                }
            }
        }
    }
}
