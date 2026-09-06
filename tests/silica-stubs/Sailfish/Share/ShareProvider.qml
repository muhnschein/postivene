import QtQuick 2.0

// What the platform triggers when something is shared to this app. The
// real one registers the method named here against the group of the same
// name in the desktop file; this one only carries what a test hands it,
// which is enough to drive everything on this side of it.
Item {
    property string method: ""
    property var capabilities: []
    property bool registerName: false

    signal triggered(var resources)
}
