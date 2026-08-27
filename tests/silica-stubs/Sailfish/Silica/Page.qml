import QtQuick 2.0

// `pageStack` is not declared here: it is a context property, which the
// harness injects.
Item {
    property bool allowedOrientations
    property int orientation
    property string backNavigation
    width: 540
    height: 960
}
