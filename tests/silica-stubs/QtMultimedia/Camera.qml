import QtQuick 2.0

// The camera. Nothing is captured here; what a test can check is that a
// page starts and stops it with its own status, wires it to a viewfinder,
// asks it to focus, and drives its still and video capture through the
// two objects below. The enums the pages name (Camera.FocusContinuous
// and the like) are the real type's and read as undefined here.
QtObject {
    property bool running: false
    property var captureMode
    property var position
    property CameraFocus focus: CameraFocus {}
    property CameraCapture imageCapture: CameraCapture {}
    property CameraRecorder videoRecorder: CameraRecorder {}
    /// How many focus searches were asked for.
    property int searches: 0
    function start() { running = true }
    // Stopping the camera stops a recording with it, finished or not: what
    // the page falls back on when the recorder ignores its own stop.
    function stop() {
        running = false
        if (videoRecorder.recorderState === 1) {
            videoRecorder.recorderState = 0
            videoRecorder.recorderStatus = 3
        }
    }
    function searchAndLock() { searches += 1 }
    function unlock() { }
}
