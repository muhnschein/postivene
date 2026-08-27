import QtQuick 2.0

// Silica's `EnterKey` attached property is missing and cannot be stubbed:
// QML forbids capitalised property names and qmetaobject cannot register
// attached types. Pages using it cannot be loaded here.
Item {
    property string text: ""
    property string label
    property string placeholderText
    property string description
    property int inputMethodHints
    property bool errorHighlight: false
    property bool focus_: false
    signal clicked()
    implicitWidth: 400
    implicitHeight: 60
}
