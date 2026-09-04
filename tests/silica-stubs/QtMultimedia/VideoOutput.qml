import QtQuick 2.0

// Where a MediaPlayer's or a Camera's picture goes. Nothing is drawn
// here: what a test can check is that the page wires a source to an
// output at all, which is what `source` holds. `fillMode` takes the
// enum the page names, which reads as undefined here.
Item {
    property var source
    property var fillMode
}
