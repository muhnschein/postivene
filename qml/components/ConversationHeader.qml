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
 * Including the room a display cutout takes. A page is drawn full-screen
 * under the notch by default (`Page::cutoutMode`), and Silica's own
 * headers answer that by padding themselves; a header that does not is
 * a chat's name with a hole punched through it.
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

    /// How far down the screen the display's own cutout reaches -- a
    /// notch, a punched hole -- and nothing of this header may be drawn
    /// above.
    ///
    /// `Screen.topCutout` is a rectangle in the screen's own
    /// coordinates, so its bottom edge is what has to be cleared rather
    /// than its height. It arrived with the cutout support in Sailfish
    /// 4.6, and a release without it is a release with nothing to
    /// avoid: the name is asked for before the value.
    ///
    /// Portrait only, because this app is: a page that never turns
    /// never has the cutout anywhere but the top.
    readonly property real cutout: typeof Screen.topCutout === "undefined"
        ? 0
        : Math.max(0, Screen.topCutout.y + Screen.topCutout.height)

    width: parent ? parent.width : 0
    // The header's own band, and the cutout above it: what sits under
    // the header is anchored to the bottom of both.
    height: Theme.itemSizeLarge + root.cutout

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
        // line of its font, and a single-line label is that tall. Below
        // whatever the cutout takes, so the band the title sits in is
        // the same one it would be on a screen without one.
        y: root.cutout + Math.floor((Theme.itemSizeLarge - height) / 2)
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
