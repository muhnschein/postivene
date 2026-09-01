// 2.5 rather than 2.0 for Image.autoTransform, which is what reads a
// photo's orientation tag. Harbour allows QtQuick up to 2.6.
import QtQuick 2.5
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0

/*
 * What a message's attachment looks like, whatever kind it is.
 *
 * The core classifies every attachment from the file itself and hands back
 * a `viewType`; nothing here inspects a file, and nothing should start.
 * This picks a renderer from that answer, and falls back to naming the file
 * for the kinds nothing better can be done with.
 *
 * Its own component because MessageDelegate is already the longest file in
 * the tree, and because a preview that can be loaded on its own is one a
 * test can drive through every kind in turn.
 *
 * Laid out by bindings rather than a Column, for the same reason
 * MessageDelegate is: a positioner sizes itself in a polish pass, which
 * never runs headlessly, so a row built from one cannot be measured. Every
 * renderer sits at y 0 and is zero-high when it is not the one showing, so
 * the height is their sum.
 */
Item {
    id: root

    /// Absolute path to the file in the core's blob directory.
    property string filePath: ""
    /// What to call it.
    property string fileName: ""
    /// The core's MIME type for it, empty when it has none.
    property string fileMime: ""
    /// Size in bytes, 0 when unknown. A real, not an int: QML has no
    /// 64-bit integer and a large file overflows one.
    property real fileBytes: 0
    /// Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc,
    /// Vcard -- the core's own enum.
    property string viewType: "Text"
    /// Pixel size when the core knows it. Often 0: it reads PNG and JPEG
    /// but returned 0x0 for a valid GIF, so nothing here may divide by
    /// these without checking.
    property int imageWidth: 0
    property int imageHeight: 0
    /// A shared contact, for `Vcard`.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""
    /// How wide the bubble lets this be.
    property real contentWidth: 0

    /// The reader asked to open this in whatever handles it.
    signal openRequested()

    readonly property bool hasFile: root.filePath.length > 0
    // Encoded, not concatenated: attachments are named by whoever sent
    // them, and a "#" or a "%" in the name makes a plain "file://" + path
    // into a URL that points somewhere else, or nowhere. Per segment,
    // with encodeURIComponent: encodeURI leaves "#" and "?" alone as
    // URL syntax, which is exactly what they must not be here.
    readonly property url fileUrl: root.hasFile
        ? Qt.resolvedUrl("file://" + root.filePath.split("/")
                                         .map(encodeURIComponent).join("/"))
        : ""

    readonly property bool isStill: root.viewType === "Image"
                                    || root.viewType === "Sticker"
    readonly property bool isAnimated: root.viewType === "Gif"
    readonly property bool isVideo: root.viewType === "Video"
    readonly property bool isSound: root.viewType === "Audio"
                                    || root.viewType === "Voice"
    readonly property bool isCard: root.viewType === "Vcard"
    /// A picture of some kind.
    readonly property bool isPicture: root.isStill || root.isAnimated
    /// True when this kind reads better filling the bubble than hugging
    /// its own text, which is what the bubble sizes itself by otherwise.
    readonly property bool wantsFullWidth: root.isPicture || root.isVideo
                                           || root.isSound || root.isCard
    /// The text the fallback row shows, so the bubble can measure it
    /// without reaching inside here for the label.
    readonly property string genericText: generic.text

    /// `Audio.PlayingState`, written as its value.
    ///
    /// The enum lives on the type rather than the instance, and the
    /// headless stub that stands in for QtMultimedia is a QML component,
    /// which cannot declare one on Qt 5.6. Comparing against
    /// `Audio.PlayingState` therefore reads as `undefined` under test and
    /// silently never matches -- the play button would simply never become
    /// a pause button, and nothing would fail.
    readonly property int playingState: 1

    // Only one renderer is ever showing, and the rest measure zero. The
    // animation is inside the still image rather than beside it, so it is
    // not in this sum.
    height: still.height + video.height + sound.height
            + card.height + generic.height

    /// m:ss from milliseconds, which is what QtMultimedia reports.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    /// A file size a person can read. Decimal units, as the platform's own
    /// file manager uses.
    function readableSize(bytes) {
        if (bytes <= 0) return ""
        var units = ["B", "kB", "MB", "GB"]
        var step = 0
        var size = bytes
        while (size >= 1000 && step < units.length - 1) {
            size = size / 1000
            step++
        }
        // Whole bytes, one decimal for everything else -- "1.5 MB", not
        // "1.5 B".
        return (step === 0 ? Math.round(size) : size.toFixed(1)) + " " + units[step]
    }

    /// How tall a picture `width` wide should be.
    ///
    /// The core reports dimensions for PNG and JPEG and not for GIF, so
    /// the loaded image's own proportions are the fallback -- and a square
    /// is the fallback for that, because a height of zero is an attachment
    /// the reader cannot see at all. `PreserveAspectFit` means a square box
    /// shows the whole picture whatever shape it turns out to be; it costs
    /// some blank space and nothing else.
    function pictureHeight(width, item) {
        // The decoded picture's own proportions first, once it has been
        // decoded. The core reads its dimensions out of the file's header,
        // which are the ones before any turn the orientation tag asks for:
        // a photo taken in portrait is stored landscape and marked, and
        // measuring the row from the stored size shapes the box the wrong
        // way round. The core's answer is still what sizes the row while
        // the decode is in flight, which is what keeps it from starting
        // square and reflowing.
        if (item.implicitWidth > 0 && item.implicitHeight > 0) {
            return width * item.implicitHeight / item.implicitWidth
        }
        if (root.imageWidth > 0 && root.imageHeight > 0) {
            return width * root.imageHeight / root.imageWidth
        }
        return width
    }

    // Every picture, animated or not. A GIF is this image with the
    // animation laid over it, rather than a second renderer beside it, for
    // two reasons that both come down to AnimatedImage being unreliable
    // about what it is holding: it reports no implicit size until its
    // movie has decoded a frame -- headlessly it never does -- and if the
    // decode fails it draws nothing at all. An Image reads the first frame
    // of a GIF regardless. So the Image sizes the row and shows the still,
    // and the animation covers it when there is one to play.
    Image {
        id: still
        objectName: "attachmentImage"
        visible: root.isPicture && root.hasFile
        width: visible ? root.contentWidth : 0
        height: visible ? root.pictureHeight(width, still) : 0
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        // A camera reads its sensor out landscape however the phone is
        // held and writes which way to turn it into an EXIF tag rather
        // than into the pixels. Every other client honours the tag; Image
        // does not unless it is asked, which is why a photo sent from here
        // arrived upright for whoever received it and lay on its side in
        // our own message view.
        autoTransform: true
        source: visible ? root.fileUrl : ""

        AnimatedImage {
            id: animated
            objectName: "attachmentAnimation"
            anchors.fill: parent
            visible: root.isAnimated
            fillMode: Image.PreserveAspectFit
            // Not playing when it is not showing: a long conversation of
            // GIFs all decoding at once is the kind of thing a phone
            // notices.
            playing: visible
            source: visible ? root.fileUrl : ""
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.openRequested()
        }
    }

    // A video: the platform thumbnailer's poster frame, with a play mark
    // over it. Tapping hands it to whatever plays video, rather than
    // building a player here.
    Item {
        id: video
        objectName: "attachmentVideo"
        visible: root.isVideo && root.hasFile
        y: 0
        width: visible ? root.contentWidth : 0
        // 16:9, because the thumbnailer crops to whatever it is given and
        // the core does not report video dimensions.
        height: visible ? Math.round(width * 9 / 16) : 0

        Rectangle {
            anchors.fill: parent
            radius: Theme.paddingSmall
            color: Theme.rgba(Theme.highlightDimmerColor, 0.6)
        }

        Thumbnail {
            id: poster
            objectName: "videoThumbnail"
            anchors.fill: parent
            sourceSize.width: width
            sourceSize.height: height
            // fillMode left at the thumbnailer's own default, which crops
            // to the size asked for. Naming a value would mean naming an
            // enum on the type, which the headless stub cannot carry.
            mimeType: root.fileMime
            source: video.visible ? root.fileUrl : ""
        }

        // Over the frame rather than beside it: the frame is the control.
        Rectangle {
            anchors.centerIn: parent
            width: Theme.itemSizeSmall
            height: width
            radius: width / 2
            color: Theme.rgba("black", 0.5)

            Label {
                anchors.centerIn: parent
                color: "white"
                font.pixelSize: Theme.fontSizeLarge
                textFormat: Text.PlainText
                text: "▶"
            }
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.openRequested()
        }
    }

    // A voice message or a music file, played where it sits. The core
    // carries no duration for most files, so the player's own is the only
    // honest source -- and it only has one once the media has loaded.
    Item {
        id: sound
        objectName: "attachmentAudio"
        visible: root.isSound && root.hasFile
        y: 0
        width: visible ? root.contentWidth : 0
        // Measured rather than fixed: the two labels and the track have to
        // fit beside a button whose size is the theme's, not ours.
        height: visible
                ? Math.max(playButton.height,
                           soundName.height + soundTime.height
                           + track.height + 2 * Theme.paddingSmall)
                : 0

        Audio {
            id: player
            source: sound.visible ? root.fileUrl : ""
            // Nothing autoplays. A conversation that starts talking when
            // it is scrolled past is the worst possible behaviour on a
            // phone that may be in a pocket.
            autoPlay: false
        }

        IconButton {
            id: playButton
            objectName: "audioPlayButton"
            anchors.verticalCenter: parent.verticalCenter
            icon.source: player.playbackState === root.playingState
                         ? "image://theme/icon-m-pause"
                         : "image://theme/icon-m-play"
            onClicked: {
                if (player.playbackState === root.playingState) {
                    player.pause()
                } else {
                    player.play()
                }
            }
        }

        Label {
            id: soundName
            objectName: "audioLabel"
            anchors {
                left: playButton.right
                leftMargin: Theme.paddingSmall
                right: parent.right
                top: parent.top
                topMargin: Theme.paddingSmall
            }
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.primaryColor
            textFormat: Text.PlainText
            text: root.viewType === "Voice"
                  //: A recorded voice message, which has no useful file name.
                  ? qsTr("Voice message")
                  : root.fileName
        }

        // Position over duration, once there is a duration to be over.
        // Blank rather than "0:00" before the media has loaded: a length of
        // zero is a claim, and an empty label is not.
        Label {
            id: soundTime
            objectName: "audioTime"
            anchors {
                left: soundName.left
                right: parent.right
                top: soundName.bottom
            }
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: player.duration > 0
                  ? root.clock(player.position) + " / " + root.clock(player.duration)
                  : ""
        }

        Rectangle {
            id: track
            anchors {
                left: soundName.left
                right: parent.right
                bottom: parent.bottom
                bottomMargin: Theme.paddingSmall
            }
            height: 2
            color: Theme.rgba(Theme.secondaryColor, 0.3)

            Rectangle {
                objectName: "audioProgress"
                height: parent.height
                color: Theme.highlightColor
                width: player.duration > 0
                       ? parent.width * player.position / player.duration : 0
            }
        }
    }

    // A shared contact. The core parses the card and hands back the name,
    // the address and the colour it would give that contact; nothing here
    // reads vCard syntax.
    Item {
        id: card
        objectName: "attachmentVcard"
        visible: root.isCard
        y: 0
        width: visible ? root.contentWidth : 0
        height: visible
                ? Math.max(cardAvatar.height,
                           cardName.height + cardAddress.height
                           + 2 * Theme.paddingSmall)
                : 0

        Avatar {
            id: cardAvatar
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.itemSizeExtraSmall
            height: width
            initial: root.vcardName
            ownColor: root.vcardColor
        }

        Label {
            id: cardName
            objectName: "vcardName"
            anchors {
                left: cardAvatar.right
                leftMargin: Theme.paddingMedium
                right: parent.right
                top: parent.top
                topMargin: Theme.paddingSmall
            }
            truncationMode: TruncationMode.Fade
            color: Theme.primaryColor
            textFormat: Text.PlainText
            text: root.vcardName
        }

        Label {
            id: cardAddress
            objectName: "vcardAddress"
            anchors {
                left: cardName.left
                right: parent.right
                top: cardName.bottom
            }
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            // The card usually carries the address as the name too, and
            // saying it twice reads as a bug rather than as thoroughness.
            text: root.vcardAddr === root.vcardName ? "" : root.vcardAddr
        }
    }

    // Everything else that has a file: named, sized, and handed to the
    // system on a tap. Webxdc apps land here -- Postivene cannot run one,
    // and a row that pretended otherwise would be worse than this.
    Label {
        id: generic
        objectName: "attachmentLabel"
        visible: root.hasFile && !root.isStill && !root.isAnimated
                 && !root.isVideo && !root.isSound && !root.isCard
        y: 0
        height: visible ? implicitHeight : 0
        width: root.contentWidth
        truncationMode: TruncationMode.Fade
        color: Theme.highlightColor
        textFormat: Text.PlainText
        text: {
            // A .vcf the core would not call a Vcard still says what
            // it is. It declines the classification when the card
            // holds no email address or more than one contact -- a
            // phone-only contact exported from the address book is
            // both common and not someone Delta Chat can open a chat
            // with -- and a paperclip makes that look like a mystery
            // blob rather than a contact this app cannot use.
            var mark = root.viewType === "Webxdc" ? "⚙"
                       : root.fileMime === "text/vcard" ? "📇" : "📎"
            var name = root.fileName.length > 0 ? root.fileName : root.filePath
            var size = root.readableSize(root.fileBytes)
            return mark + " " + name + (size.length > 0 ? " (" + size + ")" : "")
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.openRequested()
        }
    }
}
