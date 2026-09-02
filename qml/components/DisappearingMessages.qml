import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * How long messages in a chat live: the same eight choices the reference
 * clients offer, from off to five weeks.
 *
 * The core is what decides. The chosen index is bound from it rather than
 * held here, and a tap asks the core for the other value; the binding then
 * shows whatever the core says -- the same arrangement as the read-receipt
 * switch, and for the same reason. Silica's ComboBox writes currentIndex
 * itself on a tap, which detaches the binding, so the page puts it back on
 * every load.
 */
Column {
    id: root

    /// Seconds, as the core holds them. 0 is off.
    property int seconds: 0
    /// Whether a choice is offered at all.
    property bool canChange: true

    /// The reader picked a duration, in seconds.
    signal chosen(int seconds)

    // In the reference clients' order.
    readonly property var choices: [0, 60, 300, 1800, 3600, 86400, 604800, 3024000]

    function labelFor(index) {
        switch (index) {
        case 0: return qsTr("Off")
        case 1: return qsTr("After 1 minute")
        case 2: return qsTr("After 5 minutes")
        case 3: return qsTr("After 30 minutes")
        case 4: return qsTr("After 1 hour")
        case 5: return qsTr("After 1 day")
        case 6: return qsTr("After 1 week")
        default: return qsTr("After 5 weeks")
        }
    }

    function indexOf(value) {
        for (var i = 0; i < choices.length; i++) {
            if (choices[i] === value) {
                return i
            }
        }
        return -1
    }

    /// Put the choice back to what the core holds.
    function refresh() {
        combo.currentIndex = root.indexOf(root.seconds)
    }

    onSecondsChanged: refresh()
    Component.onCompleted: refresh()

    width: parent ? parent.width : 0

    ComboBox {
        id: combo
        objectName: "disappearingCombo"
        width: parent.width
        label: qsTr("Disappearing messages")
        enabled: root.canChange
        // A duration another client set that is not on the list is still
        // said, in seconds, rather than shown as nothing.
        value: currentIndex >= 0
               ? root.labelFor(currentIndex)
               //: A disappearing-messages duration not among the offered ones. %n is seconds.
               : qsTr("After %n second(s)", "", root.seconds)

        menu: ContextMenu {
            Repeater {
                model: root.choices

                MenuItem {
                    objectName: "timerOption" + modelData
                    text: root.labelFor(index)
                    onClicked: root.chosen(modelData)
                }
            }
        }
    }

    Label {
        objectName: "disappearingNote"
        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: qsTr("Applies to all members of this chat, they can still copy, save, and forward messages.")
    }
}
