import QtQuick 2.0

// The camera. Nothing is captured here; what a test can check is that the
// page starts and stops it with its own status, wires it to a viewfinder,
// and asks it to focus. The enums the page names (Camera.FocusContinuous
// and the like) are the real type's and read as undefined here.
QtObject {
    property bool running: false
    property var captureMode
    property CameraFocus focus: CameraFocus {}
    /// How many focus searches were asked for.
    property int searches: 0
    function start() { running = true }
    function stop() { running = false }
    function searchAndLock() { searches += 1 }
    function unlock() { }
}
