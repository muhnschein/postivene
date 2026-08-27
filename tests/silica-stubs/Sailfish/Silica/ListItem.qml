import QtQuick 2.0
Item {
    property int contentHeight: 80
    property bool down: false
    property bool menuOpen: false
    signal clicked()
    width: parent ? parent.width : 540
    height: contentHeight
}
