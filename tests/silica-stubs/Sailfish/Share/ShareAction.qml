import QtQuick 2.0

// The other side of sharing: what an app uses to hand something to
// somebody else. Nothing here uses it yet; it is stubbed because the
// module it comes from is stubbed.
QtObject {
    property string title: ""
    property string mimeType: ""
    property var resources: []

    function trigger() {}
    function loadConfiguration(configuration) {}
}
