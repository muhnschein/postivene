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
 * Three choices, nearest the thumb first: the camera, for a picture or
 * a video taken now; the paper clip, which is the platform's own picker
 * over everything the phone has indexed -- pictures, videos, music,
 * documents -- rather than one entry per kind; and the microphone, for
 * a voice message, offered only where something can record one.
 *
 * Nothing is opened here. The page that owns the pageStack pushes the
 * pickers and starts the recording, the way ConversationPage already
 * handles forwarding -- which keeps this component loadable, and
 * testable, on its own. The page also closes the tray: a tap anywhere
 * else on it is a tap that did not mean the tray.
 */
Item {
    id: root

    /// Whether the tray is showing.
    property bool open: false
    /// Whether a voice message can be recorded here. Without it the
    /// microphone is not offered at all.
    property bool voiceAvailable: false
    /// A picture or a video, taken now.
    signal cameraRequested()
    /// Something the phone has indexed: a picture, a video, a song, a
    /// document.
    signal libraryRequested()
    /// A voice message, recorded now.
    signal voiceRequested()

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
                objectName: "attachCamera"
                icon.source: "image://theme/icon-m-camera"
                onClicked: {
                    root.close()
                    root.cameraRequested()
                }
            }

            IconButton {
                objectName: "attachLibrary"
                icon.source: "image://theme/icon-m-attach"
                onClicked: {
                    root.close()
                    root.libraryRequested()
                }
            }

            IconButton {
                objectName: "attachVoice"
                visible: root.voiceAvailable
                icon.source: "image://theme/icon-m-mic"
                onClicked: {
                    root.close()
                    root.voiceRequested()
                }
            }
        }
    }
}
