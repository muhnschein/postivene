import QtQuick 2.0

// Silica's flickable sizes its content to its own width; Qt's leaves
// contentWidth at 0, which makes anything anchored to `parent` inside it
// zero pixels wide and stops a list in there from building any delegates
// at all. Without this the stub silently disagrees with the device.
Flickable {
    property bool pressDelay
    property var pullDownMenu
    contentWidth: width
}
