import QtQuick 2.0

// Note what is *missing*: Silica's `EnterKey` attached property. QML forbids
// property names beginning with a capital letter, and qmetaobject cannot
// register an attached type, so no stub can provide it -- a page using
// `EnterKey.onClicked:` cannot be loaded by this harness at all. The
// onboarding pages therefore do without it; see the note in
// rust/postivene-shim/tests/qml_pages.rs.
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
