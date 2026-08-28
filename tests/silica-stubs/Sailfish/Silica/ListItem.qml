import QtQuick 2.0
Item {
    property int contentHeight: 80
    property bool down: false
    property bool menuOpen: false
    property var menu
    signal clicked()
    // Silica asks for confirmation with a countdown; the stub just runs it.
    function remorseAction(text, action) { action() }
    width: parent ? parent.width : 540
    height: contentHeight
}
