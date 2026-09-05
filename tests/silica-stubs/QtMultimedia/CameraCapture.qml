import QtQuick 2.0

// The camera's still capture. Nothing is written: a test drives the
// answer by calling `saved` with the path the page asked for, the way
// the real one signals once the file is on disk.
QtObject {
    property bool capturing: false
    property bool ready: true
    /// Where the last capture was asked to go.
    property string requested: ""
    signal imageSaved(int requestId, string path)
    signal captureFailed(int requestId, string message)
    function captureToLocation(location) {
        requested = location
        return 1
    }
    function capture() { return 1 }
    /// The file the page asked for has been written.
    function saved() { imageSaved(1, requested) }
}
