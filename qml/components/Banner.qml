import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A strip along the page saying what just happened -- a failure, or a
 * confirmation. Transient: a message the reader cannot act on should not
 * sit there forever, and the page clears its own state on `dismissed`
 * rather than having this write to it.
 */
Item {
    id: root

    property string text: ""
    // "error" or "info": what happened, and how loudly to say it.
    property string tone: "error"
    // Seconds before the message clears itself; 0 keeps it.
    property int timeout: 8
    signal dismissed()

    readonly property color toneColor: root.tone === "error" ? Theme.errorColor
                                                             : Theme.highlightColor

    /// Say something for a while. For the cases the page has no state for.
    function show(message) {
        root.text = message
    }

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
        color: Theme.rgba(root.toneColor, 0.15)

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
            textFormat: Text.PlainText
            text: root.text
            color: root.toneColor
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
