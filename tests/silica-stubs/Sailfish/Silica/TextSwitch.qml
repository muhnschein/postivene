import QtQuick 2.0

// Silica's labelled switch.
Item {
    property string text
    property string description
    property bool checked: false
    property bool automaticCheck: true
    signal clicked()
    implicitWidth: 400
    implicitHeight: 80
}
