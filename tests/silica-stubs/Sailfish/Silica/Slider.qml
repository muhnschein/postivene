import QtQuick 2.0

// Silica's slider, cut down to what the video page uses. `down` is true
// while the handle is held, which is what keeps the position binding off
// the value the reader is dragging.
Item {
    id: root

    property real minimumValue: 0
    property real maximumValue: 1
    property real value: 0
    property real stepSize: 0
    property string label
    property string valueText
    property bool down: false
    property bool handleVisible: true

    signal released()

    width: parent ? parent.width : 540
    height: 80
}
