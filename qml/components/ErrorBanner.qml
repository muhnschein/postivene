import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Where a failure is shown. Transient: a message the user cannot act on
 * should not sit on the page forever, and the page clears its own state on
 * `dismissed` rather than having this write to it.
 */
Item {
    id: root

    property string text: ""
    // Seconds before the message clears itself; 0 keeps it.
    property int timeout: 8
    signal dismissed()

    width: parent ? parent.width : 0
    height: root.text.length > 0 ? strip.height : 0
    visible: root.text.length > 0

    onTextChanged: {
        if (root.text.length > 0 && root.timeout > 0) {
            fade.restart()
        } else {
            fade.stop()
        }
    }

    Rectangle {
        id: strip
        width: parent.width
        height: label.implicitHeight + 2 * Theme.paddingMedium
        color: Theme.rgba(Theme.errorColor, 0.15)

        Label {
            id: label
            objectName: "errorLabel"
            anchors {
                left: parent.left
                right: parent.right
                verticalCenter: parent.verticalCenter
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
            }
            text: root.text
            color: Theme.errorColor
            font.pixelSize: Theme.fontSizeSmall
            wrapMode: Text.Wrap
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismissed()
        }
    }

    Timer {
        id: fade
        interval: root.timeout * 1000
        onTriggered: root.dismissed()
    }
}
