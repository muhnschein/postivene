import QtQuick 2.0

// The camera. Nothing is captured here; what a test can check is that the
// page starts and stops it with its own status, and wires it to a
// viewfinder.
QtObject {
    property bool running: false
    function start() { running = true }
    function stop() { running = false }
}
