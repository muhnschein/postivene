import QtQuick 2.0

// Silica's multi-line field. Like its TextField, but it grows with what
// is typed into it rather than scrolling one line sideways, and the
// return key puts in a line break instead of accepting.
//
// `implicitHeight` grows by a line per line of text, which is the whole
// of what the conversation binds to: it caps the field's height at a
// share of the page, and a stub that reported one fixed height could not
// show that cap doing anything.
Item {
    property string text: ""
    property string label
    property string placeholderText
    property string description
    property int inputMethodHints
    property bool errorHighlight: false
    property bool readOnly: false
    property bool labelVisible: true
    property int horizontalAlignment: 0
    property int textTopMargin: 0
    property font font
    property color color: "#ffffff"
    property color placeholderColor: "#a0a0a0"
    signal clicked()
    implicitWidth: 400
    // One line, plus one for every line break in what is there.
    implicitHeight: 60 + 40 * (text.split("\n").length - 1)
}
