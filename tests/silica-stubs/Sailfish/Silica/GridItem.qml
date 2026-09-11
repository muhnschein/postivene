import QtQuick 2.0

// Silica's grid cell with a menu on it: what ListItem is to a list, this
// is to a grid. Sized by `contentWidth` and `contentHeight`, which Silica
// reads off the view's cells; the menu is built and found the way the
// ListItem stub builds and finds its own.
Item {
    id: root

    property real contentWidth: 180
    property real contentHeight: 180
    property bool down: false
    property bool highlighted: down
    property bool menuOpen: false
    property bool showMenuOnPressAndHold: true
    property var menu
    onMenuChanged: {
        if (menu && typeof menu.createObject === "function") {
            menu = menu.createObject(root)
        }
    }
    signal clicked()
    function openMenu() { menuOpen = true }
    function closeMenu() { menuOpen = false }

    width: contentWidth
    height: contentHeight
}
