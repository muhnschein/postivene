import QtQuick 2.0
import QtGraphicalEffects 1.0
import Sailfish.Silica 1.0

/*
 * A round avatar: the subject's own colour with their initial on it, or
 * their picture drawn through a circular mask.
 *
 * The mask is the whole point. An Image does not inherit its parent's
 * corner radius, and `clip` cuts only to the bounding box, so a picture
 * left to itself reads as a square among circles.
 *
 * Shared by the chat list and the contact lists so the two cannot drift.
 */
Rectangle {
    id: avatar

    /// Shown when there is no picture.
    property string initial
    /// The core's per-chat or per-contact colour.
    property string ownColor
    /// Path to the picture, empty when there is none.
    property string picturePath
    /// Drawn without its colour: grey behind the initial, the picture
    /// desaturated. For the cover's grid of everyone, where colour is
    /// kept for whoever has something new.
    property bool monochrome: false

    width: Theme.itemSizeSmall
    height: width
    radius: width / 2
    // The disc's own colour only when it is what is seen: behind a
    // picture it showed as a tinted ring where the mask's edge is
    // softened, and as a full tinted disc whenever the masked picture
    // was not drawn for a frame -- a row highlighted under its context
    // menu was where that was noticed.
    color: avatar.picturePath.length > 0 ? "transparent"
           : avatar.monochrome ? Theme.rgba(Theme.primaryColor, 0.25)
           : ownColor.length > 0 ? ownColor : Theme.highlightColor

    Label {
        objectName: "avatarInitial"
        anchors.centerIn: parent
        visible: avatar.picturePath.length === 0
        color: Theme.primaryColor
        font.pixelSize: Theme.fontSizeLarge
        textFormat: Text.PlainText
        text: avatar.initial.substring(0, 1).toUpperCase()
    }

    Image {
        id: picture
        objectName: "avatarImage"
        anchors.fill: parent
        visible: false
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Encoded per segment; see AttachmentPreview.qml's fileUrl.
        source: avatar.picturePath.length > 0
                ? Qt.resolvedUrl("file://" + avatar.picturePath.split("/")
                                                 .map(encodeURIComponent).join("/"))
                : ""
    }

    Rectangle {
        id: mask
        objectName: "avatarMask"
        anchors.fill: parent
        radius: width / 2
        visible: false
    }

    OpacityMask {
        objectName: "avatarMasked"
        anchors.fill: parent
        visible: avatar.picturePath.length > 0
        source: picture
        maskSource: mask
        // An effect re-runs its shader whenever what it draws is redrawn,
        // and a list redraws its rows on every frame of a scroll. An
        // avatar does not change between those frames, so the result is
        // kept in a texture and reused until the picture does change.
        cached: true
        // The colour taken out on the way to the screen, as a layer over
        // the masked picture, so the mask and the desaturation are one
        // texture rather than two effects drawn over each other.
        layer.enabled: avatar.monochrome
        layer.effect: Desaturate {
            desaturation: 1.0
        }
    }
}
