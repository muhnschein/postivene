import QtQuick 2.0

// Silica's grid is Qt's with a pulley on it. The delegates it builds are
// real, so a test can find a tile by name and tap it.
GridView {
    property var pullDownMenu
    property var pushUpMenu
}
