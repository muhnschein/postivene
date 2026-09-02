import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A page header for a title the other end chose.
 *
 * Silica's PageHeader draws its title in a Label of its own and offers no
 * textFormat, so a chat named `<img src="https://tracker/p.gif">` would
 * be markup to it, and drawing the header would fetch the image -- from
 * an app whose whole point is that the network cannot watch. This is the
 * same header, the size and margin and highlight and right-aligned fade,
 * with its one label pinned to plain text.
 *
 * It also keeps out of the display cutout. A short title never reached
 * it; a long one, fading on the left, ran straight under the notch. The
 * title is drawn below the cutout, by its height, which is the one thing
 * about it that can be trusted: placing the text beside the notch needed
 * its position, and what Screen.topCutout says about that put a long
 * name at the left edge, truncated, on the device this was written for.
 */
Item {
    id: root

    /// What the header says. Shown as written, whatever it looks like.
    property alias title: titleLabel.text

    /// The header was tapped. What that opens is the page's to decide.
    signal clicked()

    // The notch, as Silica describes it. A `var` rather than a `rect`:
    // a screen without one, or a Silica without the property, then reads
    // as no inset instead of as a binding error.
    readonly property var cutout: Screen.topCutout
    /// How far down the cutout reaches, 0 without one.
    readonly property real cutoutInset:
        cutout && cutout.height > 0 ? Math.max(0, cutout.y + cutout.height) : 0

    width: parent ? parent.width : 0
    // Taller by the inset, so what sits below the header moves with it.
    height: Theme.itemSizeLarge + cutoutInset

    MouseArea {
        objectName: "headerTap"
        anchors.fill: parent
        onClicked: root.clicked()
    }

    Label {
        id: titleLabel
        objectName: "headerTitle"
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            // Centred in the part of the header below the cutout.
            verticalCenter: parent.verticalCenter
            verticalCenterOffset: root.cutoutInset / 2
        }
        horizontalAlignment: Text.AlignRight
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
