import QtQuick 2.0
Item {
    id: root

    property int contentHeight: 80
    property bool down: false
    property bool highlighted: down
    property bool menuOpen: false
    property var menu
    // Silica takes a Component here as well as a menu, and builds the
    // Component the first time the menu is opened. The tests look inside
    // a row's menu the moment the row exists, so a Component is built
    // here as soon as it is set, in the scope it was declared in -- which
    // is what Silica's own build gives it.
    onMenuChanged: {
        if (menu && typeof menu.createObject === "function") {
            menu = menu.createObject(root)
        }
    }
    signal clicked()
    // Silica opens the row's context menu on a long press, and offers
    // this for anything that took the press itself and wants the same.
    function openMenu() { menuOpen = true }

    // No `remorseAction`. Silica has one and this stub used to model it,
    // guessing at what it does when the row is destroyed mid-countdown --
    // which is exactly the case that matters, and exactly the one a stub
    // cannot answer for. Nothing in qml/ calls it any more: a wait before
    // something is destroyed belongs to the list (components/
    // PendingRemoval.qml), so a row going away is not part of it. Left out
    // rather than left in, so anything that reaches for it again fails
    // here rather than on a phone.

    width: parent ? parent.width : 540
    height: contentHeight
}
