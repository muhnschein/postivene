import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Point the camera at a QR code. What the code says is handed back on
 * `scanned` and the page goes; what to do with it is the caller's --
 * an invite is followed, a provider payload creates a profile -- and the
 * core is asked what it is first, the same as for a pasted link.
 *
 * A page of its own, pushed by URL and connected to, the way the pickers
 * are: this is the only file that names a Camera, so a device without
 * one costs a button rather than the page that has it.
 *
 * Frames come through `grabToImage`, which is the one way QML on Qt 5.6
 * hands a viewfinder's pixels to anything, and go to the scanner as a
 * file. The next frame is grabbed only once the last has been decoded, so
 * a slow decode never queues frames behind it.
 *
 * Focus is asked for three ways, because a camera left to itself gave a
 * viewfinder too soft for any code to register: continuous autofocus in
 * the video pipeline, where the platform's cameras run it; a focus search
 * every couple of seconds while nothing has been read, for a camera that
 * only focuses when told; and a tap on the viewfinder, which focuses on
 * the point under the finger.
 */
Page {
    id: page

    /// A code was read, and this is its text.
    signal scanned(string text)

    property string errorMessage: ""
    /// Set once a code is read, so a second frame in flight cannot
    /// report the same code twice.
    property bool done: false

    QrScanner {
        id: scanner
        objectName: "scanner"
        onFound: {
            if (page.done) {
                return
            }
            page.done = true
            camera.stop()
            page.scanned(text)
            pageStack.pop()
        }
        onError: page.errorMessage = message
    }

    Camera {
        id: camera
        objectName: "camera"
        // The video pipeline is where continuous autofocus runs.
        captureMode: Camera.CaptureVideo
        focus {
            focusMode: Camera.FocusContinuous
            focusPointMode: Camera.FocusPointAuto
        }
    }

    // An autofocus run, for a camera that does not run one on its own.
    // Unlocked first: a search that finds focus locks it, and a locked
    // focus is a fixed one.
    function refocus() {
        camera.unlock()
        camera.searchAndLock()
    }

    // Every couple of seconds while nothing has been read.
    Timer {
        id: refocusTimer
        objectName: "refocus"
        interval: 2500
        repeat: true
        triggeredOnStart: true
        running: page.status === PageStatus.Active && !page.done
        onTriggered: page.refocus()
    }

    // The camera runs only while this page is the one on screen.
    onStatusChanged: {
        if (page.status === PageStatus.Active) {
            camera.start()
        } else if (page.status === PageStatus.Deactivating) {
            camera.stop()
        }
    }

    VideoOutput {
        id: viewfinder
        objectName: "viewfinder"
        anchors.fill: parent
        source: camera

        // Tap to focus on what is under the finger.
        MouseArea {
            objectName: "focusTap"
            anchors.fill: parent
            onClicked: {
                camera.focus.focusPointMode = Camera.FocusPointCustom
                camera.focus.customFocusPoint = Qt.point(mouse.x / width, mouse.y / height)
                page.refocus()
            }
        }
    }

    // Grabbed small: a code fills a good part of the frame when someone
    // is holding a phone up to it, and a third of the pixels decode in a
    // ninth of the time.
    Timer {
        id: grabber
        objectName: "grabber"
        interval: 300
        repeat: true
        running: page.status === PageStatus.Active && !scanner.busy && !page.done
        onTriggered: {
            var path = scanner.frame_path()
            if (path.length === 0) {
                return
            }
            viewfinder.grabToImage(function(result) {
                if (result.saveToFile(path)) {
                    scanner.decode(path)
                }
            }, Qt.size(640, 640))
        }
    }

    PageHeader {
        title: qsTr("Scan QR code")
    }

    Label {
        objectName: "hint"
        anchors {
            left: parent.left
            right: parent.right
            bottom: banner.top
            margins: Theme.horizontalPageMargin
        }
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        color: Theme.secondaryHighlightColor
        text: qsTr("Point the camera at an invite or a chatmail server code")
    }

    Banner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
