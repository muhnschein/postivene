import QtQuick 2.0

// Stands in for Nemo.Notifications' Notification, which only exists on a
// device. It imitates no behaviour beyond recording what it was asked to
// do, so a test can see whether a notification was raised and taken back
// down again -- see tests/silica-stubs/Sailfish/Silica for the same idea.
QtObject {
    property string summary
    property string body
    property string previewSummary
    property string previewBody
    property string category
    property int replacesId: 0

    // What the tests read.
    property bool isPublished: false
    property int publishCount: 0
    property int closeCount: 0

    function publish() {
        isPublished = true
        publishCount += 1
        // The real one allocates an id on first publish and reuses it.
        if (replacesId === 0) {
            replacesId = publishCount
        }
    }

    function close() {
        isPublished = false
        closeCount += 1
        replacesId = 0
    }
}
