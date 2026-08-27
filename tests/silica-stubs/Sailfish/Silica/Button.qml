import QtQuick 2.0

// A label and a clicked() signal the harness can emit for a tap.
Item {
    property string text
    property bool down: false
    signal clicked()
    implicitWidth: 200
    implicitHeight: 60
}
