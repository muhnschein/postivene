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
        // The theme's own highlight carries transparency of its own, which
        // let the messages through; this sets the amount deliberately.
        color: Theme.rgba(Theme.highlightBackgroundColor, 0.5)

        // Drawn rather than themed: `icon-m-down` is itself a disc with a
        // chevron on it, which read as two circles stacked.
        Item {
            id: chevron
            objectName: "jumpChevron"
            anchors.centerIn: parent
            width: parent.width * 0.42
            height: width * 0.5

            readonly property real thickness: Math.max(2, disc.width * 0.05)
            readonly property real stroke:
                Math.sqrt(width * width / 4 + height * height)
            readonly property real slope:
                Math.atan2(height, width / 2) * 180 / Math.PI

            Rectangle {
                x: 0
                y: 0
                width: chevron.stroke
                height: chevron.thickness
                radius: height / 2
                color: Theme.primaryColor
                antialiasing: true
                transformOrigin: Item.TopLeft
                rotation: chevron.slope
            }

            Rectangle {
                x: chevron.width
                y: 0
                width: chevron.stroke
                height: chevron.thickness
                radius: height / 2
                color: Theme.primaryColor
                antialiasing: true
                transformOrigin: Item.TopLeft
                rotation: 180 - chevron.slope
            }
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
