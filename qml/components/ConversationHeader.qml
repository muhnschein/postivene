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
 *
 * On a screen with a cutout, the title is also kept to the right-hand
 * part of the header, which is the part a right-aligned title is for.
 * PageHeader itself does nothing about a cutout, and its titles are short
 * enough never to reach one; a chat's name is not, and fading on the
 * left it ran straight under the notch. Where the notch is comes from the
 * device in a shape this tree cannot read, and two attempts at using it
 * put the title at the left edge and then too low. What is left is what
 * does not depend on it: a title that stays on its own side.
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

    // A `var` rather than a `rect`: a screen without a cutout, or a Silica
    // without the property, then reads as none rather than as an error.
    readonly property var cutout: Screen.topCutout
    readonly property bool hasCutout: !!cutout && cutout.height > 0
    /// The room the title may take: the width less the margins, or with
    /// a cutout the right-hand part of it.
    readonly property real titleRoom:
        hasCutout ? Math.floor(root.width * 0.45)
                  : root.width - 2 * Theme.horizontalPageMargin

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
        // No wider than its text, and no wider than the room.
        width: Math.min(implicitWidth, root.titleRoom)
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
