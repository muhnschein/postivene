import QtQuick 2.0
import Sailfish.Silica 1.0

// A page that can be accepted. Silica accepts on the header's tap or a
// forward swipe and then goes to `acceptDestination` with
// `acceptDestinationProperties`; here the harness calls `accept()`, and
// the push is recorded by the injected `pageStack` like any other.
Page {
    property bool canAccept: true
    property var acceptDestination
    property var acceptDestinationProperties
    signal accepted()
    signal rejected()
    function accept() {
        if (!canAccept) {
            return
        }
        accepted()
        if (acceptDestination) {
            pageStack.push(acceptDestination, acceptDestinationProperties || {})
        }
    }
    function reject() { rejected() }
}
