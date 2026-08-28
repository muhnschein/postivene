import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Back to the newest message, from further up the history. A round button
 * rather than a labelled one: it sits over the conversation, and a chevron
 * needs no translating.
 */
Item {
    id: root

    property int count: 0
    signal clicked()

    width: Theme.itemSizeSmall
    height: width

    Rectangle {
        id: disc
        objectName: "jumpDisc"
        anchors.fill: parent
        radius: width / 2
        // Opaque, taking only the hue: the theme's own highlight is part
        // transparent, and the messages behind showed through it.
        color: Theme.rgba(Theme.highlightBackgroundColor, 1.0)

        Image {
            objectName: "jumpIcon"
            anchors.centerIn: parent
            source: "image://theme/icon-m-down"
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.clicked()
        }
    }

    // How many arrived while the reader was away.
    Rectangle {
        id: badge
        objectName: "jumpBadge"
        visible: root.count > 0
        width: visible ? Math.max(height, badgeLabel.implicitWidth + Theme.paddingSmall) : 0
        height: visible ? badgeLabel.implicitHeight + Theme.paddingSmall : 0
        anchors {
            right: parent.right
            top: parent.top
        }
        radius: height / 2
        color: Theme.rgba(Theme.errorColor, 1.0)

        Label {
            id: badgeLabel
            objectName: "jumpBadgeLabel"
            anchors.centerIn: parent
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.primaryColor
            text: root.count > 99 ? "99+" : root.count
        }
    }
}
