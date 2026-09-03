import QtQuick 2.0
import Sailfish.Silica 1.0

// A page that can be accepted. Silica makes `acceptDestination` as soon
// as the dialog is on screen and exposes it as `acceptDestinationInstance`
// for `onAccepted` to fill in; accepting then goes to it. Here the harness
// sets the instance to something of its own and calls `accept()`, and the
// push is recorded by the injected `pageStack` like any other.
Page {
    property bool canAccept: true
    property var acceptDestination
    property int acceptDestinationAction: 0
    property var acceptDestinationInstance
    signal accepted()
    signal rejected()
    function accept() {
        if (!canAccept) {
            return
        }
        accepted()
        if (acceptDestination) {
            pageStack.push(acceptDestination, {})
        }
    }
    function reject() { rejected() }
}
