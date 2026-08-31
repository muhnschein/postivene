import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The "+" beside the message field, and the tray it opens.
 *
 * The shape is Whisperfish's (qml/components/ChatTextInput.qml): a plus
 * that turns into a cross and lifts a small column of choices above itself,
 * rather than a row of buttons permanently crowding the field. It gets the
 * one-handed reach right -- the choices appear directly under the thumb
 * that opened them.
 *
 * Two choices, because they are the two the sandbox can serve: the gallery
 * for a photo, the file system for anything else. No camera entry until the
 * app asks for the Camera permission, which harbour-postivene.desktop
 * deliberately does not (docs/HARBOUR.md).
 *
 * Nothing is opened here. The page that owns the pageStack pushes the
 * pickers, the way ConversationPage already handles forwarding -- which
 * keeps this component loadable, and testable, on its own.
 */
Item {
    id: root

    /// Whether the tray is showing.
    property bool open: false
    signal photoRequested()
    signal fileRequested()

    function close() {
        root.open = false
    }

    width: toggle.width
    height: toggle.height

    IconButton {
        id: toggle
        objectName: "attachToggle"
        anchors.bottom: parent.bottom
        icon.source: "image://theme/icon-m-add"
        // Animated through a plain property: a Behavior cannot be attached
        // to a member of a grouped property, which `icon.rotation` is.
        property real turn: root.open ? 45 : 0
        Behavior on turn { NumberAnimation { duration: 150 } }
        icon.rotation: toggle.turn
        onClicked: root.open = !root.open
    }

    // Drawn outside this item's bounds, above the button. Nothing in the
    // input row clips, and the row is declared after the message list, so
    // it paints over the conversation.
    Rectangle {
        id: tray
        objectName: "attachTray"
        anchors {
            horizontalCenter: toggle.horizontalCenter
            bottom: toggle.top
        }
        width: toggle.width
        height: choices.height + 2 * Theme.paddingSmall
        radius: width / 4
        color: Theme.rgba(Theme.highlightDimmerColor, 0.9)

        // `enabled` follows the button rather than the fade, so a tray on
        // its way out stops taking taps at once instead of a frame before
        // it disappears. `visible` follows the fade, so it is gone rather
        // than transparent once it has.
        opacity: root.open ? 1.0 : 0.0
        visible: opacity > 0.0
        enabled: root.open
        Behavior on opacity { NumberAnimation { duration: 150 } }

        Column {
            id: choices
            anchors.centerIn: parent
            spacing: Theme.paddingSmall

            IconButton {
                objectName: "attachPhoto"
                icon.source: "image://theme/icon-m-image"
                onClicked: {
                    root.close()
                    root.photoRequested()
                }
            }

            IconButton {
                objectName: "attachFile"
                icon.source: "image://theme/icon-m-attach"
                onClicked: {
                    root.close()
                    root.fileRequested()
                }
            }
        }
    }
}
