import QtQuick 2.0

// Silica's drop-down: a label, the chosen item's text, and a ContextMenu
// of MenuItems. Silica sets currentIndex when an item is tapped; the
// pages here bind it from the core and act on the item's own click, so
// the stub only carries the properties.
Item {
    property string label
    property string description
    property string value
    property int currentIndex: -1
    property var currentItem
    property var menu
    implicitWidth: 400
    implicitHeight: 80
}
