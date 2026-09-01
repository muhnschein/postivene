import QtQuick 2.0

// Where a MediaPlayer's picture goes. Nothing is drawn here: what a test
// can check is that the page wires a player to an output at all, which is
// what `source` holds.
Item {
    property var source
}
