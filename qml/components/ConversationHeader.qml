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
 */
Item {
    id: root

    /// What the header says. Shown as written, whatever it looks like.
    property alias title: titleLabel.text

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
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        horizontalAlignment: Text.AlignRight
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
