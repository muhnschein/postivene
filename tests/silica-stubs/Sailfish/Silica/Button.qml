import QtQuick 2.0

// Only what a test needs: a label, an enabled flag, and a clicked() signal
// the harness can emit to stand in for a tap.
Item {
    property string text
    property bool down: false
    signal clicked()
    implicitWidth: 200
    implicitHeight: 60
}
