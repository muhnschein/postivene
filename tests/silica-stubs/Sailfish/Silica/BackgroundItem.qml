import QtQuick 2.0

// Silica's tappable row background: an Item with a press state and a
// clicked() signal. `down` is what a label reads to highlight itself.
Item {
    id: root

    property bool down: mouse.pressed
    property bool highlighted: down
    signal clicked()

    width: parent ? parent.width : 540
    height: 80

    MouseArea {
        id: mouse
        anchors.fill: parent
        onClicked: root.clicked()
    }
}
