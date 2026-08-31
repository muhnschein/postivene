import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The file the next message will carry, above the field.
 *
 * Its own component for the same reason ReplyBar is: ConversationPage
 * cannot be loaded headlessly, so anything whose measurement matters has to
 * be testable on its own. It also deliberately looks like ReplyBar -- both
 * say "the next message will have this in it", and one idiom for that is
 * easier to read than two.
 */
Item {
    id: root

    /// The picked file, empty for none. This is what makes the bar appear.
    property string filePath
    /// What to call it. Falls back to the path when a picker gave no name.
    property string fileName
    signal cancelled()

    visible: root.filePath.length > 0
    // Both, not just the label: the cancel button is an icon's worth tall.
    // Same reasoning as ReplyBar, where a one-line quote measured short and
    // the bar overlapped the field below it.
    height: visible ? Math.max(attached.height, cancel.height) + 2 * Theme.paddingSmall : 0

    Label {
        id: attached
        objectName: "pendingAttachmentLabel"
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - x - cancel.width - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        //: Shown above the message field once a file has been picked. %1 is
        //: the file name.
        text: qsTr("Sending %1").arg(
                  root.fileName.length > 0 ? root.fileName : root.filePath)
    }

    IconButton {
        id: cancel
        objectName: "cancelAttachmentButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.cancelled()
    }
}
