import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * What the next message answers, above the field. Its own component so the
 * wrapping can be measured: ConversationPage cannot be loaded headlessly.
 */
Item {
    id: root

    property string author
    property string body
    // Long enough to recognise the message, short enough not to take the
    // screen; what does not fit ends in an ellipsis.
    property int maximumLines: 3
    signal cancelled()

    visible: root.body.length > 0 || root.author.length > 0
    height: visible ? quoted.height + 2 * Theme.paddingSmall : 0

    Label {
        id: quoted
        objectName: "replyLabel"
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - x - cancel.width - Theme.paddingMedium
        wrapMode: Text.Wrap
        maximumLineCount: root.maximumLines
        truncationMode: TruncationMode.Elide
        elide: Text.ElideRight
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        // One `arg`, then joined on: QML's takes a single argument, and
        // a second call would rescan what the first produced -- so a
        // contact named "%2" would get the body put where they belong.
        text: qsTr("Replying to %1").arg(root.author) + ": " + root.body
    }

    IconButton {
        id: cancel
        objectName: "cancelReplyButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.cancelled()
    }
}
