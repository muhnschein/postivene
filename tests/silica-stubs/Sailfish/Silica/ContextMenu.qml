import QtQuick 2.0
Item {
    default property alias contents: holder.data
    Item { id: holder }
    // Silica closes the menu itself when a MenuItem is tapped; anything
    // else in the menu has to ask. Nothing to close here, but the call has
    // to resolve or a handler that makes it stops at that line.
    function close() {}
}
