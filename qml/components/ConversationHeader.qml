import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A page header for a title the other end chose.
 *
 * Silica's PageHeader draws its title in a Label of its own and offers no
 * textFormat, so a chat named `<img src="https://tracker/p.gif">` would
 * be markup to it, and drawing the header would fetch the image -- from
 * an app whose whole point is that the network cannot watch. This is the
 * same header, laid out as PageHeader lays out its own: the title on the
 * line the page indicator sits on, right-aligned, no wider than the room
 * it has, in the page's own colour when the header leads somewhere and
 * the highlight when it does not -- with its one label pinned to plain
 * text.
 */
Item {
    id: root

    /// What the header says. Shown as written, whatever it looks like.
    property alias title: titleLabel.text

    /// Whether the header leads to another page. Drawn as PageHeader
    /// draws a header of a page that can navigate forward.
    property bool interactive: false

    /// The header was tapped. What that opens is the page's to decide.
    signal clicked()

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
        // No wider than its text, and no wider than the page less its
        // margins.
        width: Math.min(implicitWidth, root.width - 2 * Theme.horizontalPageMargin)
        // The line PageHeader puts its first line on: it measures one
        // line of its font, and a single-line label is that tall.
        y: Math.floor((Theme.itemSizeLarge - height) / 2)
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        horizontalAlignment: Text.AlignRight
        color: root.interactive ? Theme.primaryColor : Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        font.family: Theme.fontFamilyHeading
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
