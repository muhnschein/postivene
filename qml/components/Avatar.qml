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

    width: Theme.itemSizeSmall
    height: width
    radius: width / 2
    color: ownColor.length > 0 ? ownColor : Theme.highlightColor

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
        // Encoded; see MessageDelegate.qml.
        source: avatar.picturePath.length > 0
                ? Qt.resolvedUrl("file://" + encodeURI(avatar.picturePath))
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
    }
}
