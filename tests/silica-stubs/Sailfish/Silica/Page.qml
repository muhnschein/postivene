import QtQuick 2.0

// A Silica Page is an Item with a few extra properties. `pageStack` is not
// declared here: in the app it is a context property, and the test harness
// injects a recording stand-in under the same name.
Item {
    property bool allowedOrientations
    property int orientation
    property string backNavigation
    width: 540
    height: 960
}
