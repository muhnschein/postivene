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
    property bool readOnly: false
    property bool focus_: false
    property int horizontalAlignment: 0
    property bool labelVisible: true
    property int textTopMargin: 0
    property font font
    // Silica draws the text and the placeholder in its own colours, and
    // the name fields ask for others.
    property color color: "#ffffff"
    property color placeholderColor: "#a0a0a0"
    signal clicked()
    implicitWidth: 400
    implicitHeight: 60
}
