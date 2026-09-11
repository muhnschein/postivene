import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * What the next message answers, above the field -- or, while a message
 * of the reader's own is being edited, which one. Its own component so
 * the wrapping can be measured: ConversationPage cannot be loaded
 * headlessly.
 */
Item {
    id: root

    property string author
    property string body
    /// The field holds a message already sent, for the reader to change:
    /// the bar names that rather than a message being answered, and the
    /// cancel puts the field back.
    property bool editing: false
    // Long enough to recognise the message, short enough not to take the
    // screen; what does not fit ends in an ellipsis.
    property int maximumLines: 3
    signal cancelled()

    // By its own reason to be here, not by `visible`, which reads the
    // effective visibility and goes false for the whole page while
    // another is over it: see MessageDelegate.
    readonly property bool shown: root.body.length > 0 || root.author.length > 0
    visible: root.shown
    // Both, not just the label: the cancel button is an icon's worth tall,
    // which for a one-line quote is more, and measuring only the label let
    // it hang out over the message field below.
    height: root.shown ? Math.max(quoted.height, cancel.height) + 2 * Theme.paddingSmall : 0

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
        text: root.editing
              //: Above the message field while a sent message's text is
              //: being changed; the message's text follows.
              ? qsTr("Editing message") + ": " + root.body
              : qsTr("Replying to %1").arg(root.author) + ": " + root.body
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
