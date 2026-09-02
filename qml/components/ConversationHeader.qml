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
 * label takes the wider of the two spans beside the cutout, so the text
 * ends before the notch whichever side of the screen it is on.
 */
Item {
    id: root

    /// What the header says. Shown as written, whatever it looks like.
    property alias title: titleLabel.text

    /// The header was tapped. What that opens is the page's to decide.
    signal clicked()

    // The notch, in portrait pixels; all zeros on a screen without one.
    readonly property rect cutout: Screen.topCutout
    readonly property bool hasCutout: cutout.height > 0 && cutout.width > 0
    readonly property real leftSpan: cutout.x
    readonly property real rightSpan: root.width - cutout.x - cutout.width
    // Beside the cutout on its wider side, by a padding rather than the
    // page margin: the cutout is not an edge.
    readonly property real titleLeftMargin:
        hasCutout && rightSpan >= leftSpan
        ? cutout.x + cutout.width + Theme.paddingLarge
        : Theme.horizontalPageMargin
    readonly property real titleRightMargin:
        hasCutout && leftSpan > rightSpan
        ? root.width - cutout.x + Theme.paddingLarge
        : Theme.horizontalPageMargin

    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge

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
            leftMargin: root.titleLeftMargin
            right: parent.right
            rightMargin: root.titleRightMargin
            verticalCenter: parent.verticalCenter
        }
        horizontalAlignment: Text.AlignRight
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
