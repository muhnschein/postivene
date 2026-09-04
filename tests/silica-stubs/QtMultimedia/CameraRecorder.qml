import QtQuick 2.0

// The camera's video recorder. `recorderState` and `recorderStatus`
// carry QMediaRecorder's own values -- Stopped 0 / Recording 1, and
// Loaded 3 / Recording 5 / Finalizing 7 -- because the real enums live
// on the type and a QML component cannot declare one on Qt 5.6. A test
// walks the recorder through them the way the backend does: a stop
// finalises first, and only then is the file where `actualLocation`
// says.
QtObject {
    property url outputLocation
    property url actualLocation
    property int recorderState: 0
    property int recorderStatus: 3
    property int duration: 0
    function record() {
        recorderState = 1
        recorderStatus = 5
    }
    /// Stops, but does not finish: the file is not done until `finish`.
    function stop() {
        recorderState = 0
        recorderStatus = 7
    }
    /// The backend has finished writing the file where it was asked to.
    function finish() {
        actualLocation = outputLocation
        recorderStatus = 3
    }
}
