import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Postivene 1.0
import "../components"

/*
 * The camera, for a picture or a video to send.
 *
 * A page of its own, pushed by URL and connected to, the way the picker
 * pages are: it names a Camera, so a device without one costs this
 * button rather than the conversation, and it reports what was made on
 * `picked` and lets the caller decide what that means -- the same shape
 * as AttachPhotoPage. What is made goes to the captures directory
 * (Captures, capture.rs), where it waits to be sent; the core copies a
 * sent file into its own directory, and the page that sent it discards
 * the capture afterwards.
 *
 * Two modes, switched under the viewfinder the way the QR page switches
 * its two sides: a still, taken with one tap; and a video, started and
 * stopped with the same button, with the time running beside it. The
 * still is written where it is asked to go; the video is written where
 * the recorder decides to put it, which is asked for once the recorder
 * says it has finished writing -- stopping is not the end of the file.
 *
 * The camera runs only while this page is the one on screen, and the
 * page takes itself down once it has something to report: a capture is
 * the answer to the question the page asks.
 */
Page {
    id: page

    /// The absolute path of the picture or video made.
    signal picked(string path)

    /// 0 for a still, 1 for a video.
    property int mode: 0
    /// A video is being recorded.
    property bool recording: false
    /// Something has been made and reported; nothing more is taken.
    property bool done: false
    property string errorMessage: ""

    /// `CameraRecorder.RecordingState`, and `CameraRecorder.FinalizingStatus`,
    /// written as their values: the enums live on the type, and the
    /// headless stub that stands in for QtMultimedia cannot declare one
    /// on Qt 5.6. See AttachmentPreview for the same problem.
    readonly property int recordingState: 1
    readonly property int finalizingStatus: 7

    /// Where the next capture goes.
    Captures {
        id: captures
        objectName: "captures"
        onError: page.errorMessage = message
    }

    Camera {
        id: camera
        objectName: "camera"
        // The still pipeline for a picture and the video one for a
        // video; switched with the mode, so the viewfinder is the one
        // the capture will come from.
        captureMode: page.mode === 0 ? Camera.CaptureStillImage : Camera.CaptureVideo
        focus {
            focusMode: Camera.FocusContinuous
            focusPointMode: Camera.FocusPointAuto
        }

        imageCapture {
            onImageSaved: page.report(path)
            onCaptureFailed: page.errorMessage = message
        }

        videoRecorder {
            // Stopping is asked for; the file is done when the recorder
            // says so, and that is when it is reported. The status runs
            // Recording, Finalizing, Loaded; the first Loaded after a
            // recording is the file.
            onRecorderStatusChanged: page.checkVideo()
            onRecorderStateChanged: page.checkVideo()
        }
    }

    /// A picture, or the start or end of a video.
    function shutter() {
        if (page.done) {
            return
        }
        if (page.mode === 0) {
            var path = captures.new_path("photo", "jpg")
            if (path.length > 0) {
                camera.imageCapture.captureToLocation(path)
            }
        } else if (page.recording) {
            camera.videoRecorder.stop()
        } else {
            var target = captures.new_path("video", "mp4")
            if (target.length > 0) {
                camera.videoRecorder.outputLocation = Qt.resolvedUrl("file://" + target)
                camera.videoRecorder.record()
                page.recording = true
            }
        }
    }

    /// The recorder has moved: a video that has finished writing is the
    /// answer.
    function checkVideo() {
        if (!page.recording || page.done) {
            return
        }
        var recorder = camera.videoRecorder
        if (recorder.recorderState === page.recordingState
                || recorder.recorderStatus === page.finalizingStatus) {
            return
        }
        // Stopped and no longer finalising: the file is where the
        // recorder says it is.
        var where = "" + recorder.actualLocation
        if (where.length === 0) {
            return
        }
        page.recording = false
        page.report(where.indexOf("file://") === 0 ? where.substring(7) : where)
    }

    /// Hand the capture back and leave.
    function report(path) {
        if (page.done) {
            return
        }
        page.done = true
        camera.stop()
        page.picked(decodeURIComponent(path))
        // The answer given, the page goes: Silica's own pickers do the
        // same. Left to the caller otherwise it would sit under the
        // conversation with the camera off.
        pageStack.pop()
    }

    // The camera runs only while this page is the one on screen.
    onStatusChanged: {
        if (page.status === PageStatus.Active && !page.done) {
            camera.start()
        } else if (page.status !== PageStatus.Active) {
            if (page.recording) {
                camera.videoRecorder.stop()
            }
            camera.stop()
        }
    }
    Component.onCompleted: {
        if (page.status === PageStatus.Active) {
            camera.start()
        }
    }

    /// m:ss from milliseconds, which is what QtMultimedia reports.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    VideoOutput {
        id: viewfinder
        objectName: "viewfinder"
        anchors.fill: parent
        source: camera
        // Filled rather than fitted: a viewfinder with bars is a smaller
        // viewfinder, and the capture is the sensor's whole frame either
        // way.
        fillMode: VideoOutput.PreserveAspectCrop

        // Tap to focus on what is under the finger, as the scanner does.
        MouseArea {
            objectName: "focusTap"
            anchors.fill: parent
            onClicked: {
                camera.focus.focusPointMode = Camera.FocusPointCustom
                camera.focus.customFocusPoint = Qt.point(mouse.x / width, mouse.y / height)
                camera.unlock()
                camera.searchAndLock()
            }
        }
    }

    // The controls, over the bottom of the viewfinder: the mode switch,
    // the shutter, and the time while a video runs.
    Rectangle {
        id: controls
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: shutter.height + modeRow.height + 3 * Theme.paddingLarge
        color: Theme.rgba("black", 0.5)

        // Which kind, said in words, the way the QR page names its two
        // sides. Not while a video runs: the pipeline it would switch is
        // the one recording.
        Row {
            id: modeRow
            objectName: "modeRow"
            anchors {
                top: parent.top
                topMargin: Theme.paddingLarge
                horizontalCenter: parent.horizontalCenter
            }
            spacing: Theme.paddingLarge
            enabled: !page.recording

            Repeater {
                model: [qsTr("Photo"), qsTr("Video")]

                MouseArea {
                    objectName: "modeOption" + index
                    width: modeLabel.implicitWidth + 2 * Theme.paddingLarge
                    height: modeLabel.implicitHeight + 2 * Theme.paddingSmall
                    onClicked: page.mode = index

                    Label {
                        id: modeLabel
                        anchors.centerIn: parent
                        font.pixelSize: Theme.fontSizeSmall
                        color: page.mode === index ? Theme.highlightColor
                                                   : Theme.secondaryColor
                        text: modelData
                    }
                }
            }
        }

        // The shutter: a disc, red while a video runs.
        Rectangle {
            id: shutter
            objectName: "shutter"
            anchors {
                top: modeRow.bottom
                topMargin: Theme.paddingLarge
                horizontalCenter: parent.horizontalCenter
            }
            width: Theme.itemSizeMedium
            height: width
            radius: width / 2
            color: page.recording ? Theme.errorColor : Theme.primaryColor
            border.width: Theme.paddingSmall
            border.color: Theme.rgba(Theme.primaryColor, 0.5)

            // A square while recording, the way a stop button is drawn.
            Rectangle {
                anchors.centerIn: parent
                visible: page.recording
                width: parent.width / 3
                height: width
                radius: Theme.paddingSmall / 2
                color: Theme.primaryColor
            }

            MouseArea {
                objectName: "shutterTap"
                anchors.fill: parent
                enabled: !page.done
                onClicked: page.shutter()
            }
        }

        // How long the video has run, beside the shutter.
        Label {
            objectName: "recordingTime"
            visible: page.recording
            anchors {
                left: shutter.right
                leftMargin: Theme.paddingLarge
                verticalCenter: shutter.verticalCenter
            }
            color: Theme.primaryColor
            font.pixelSize: Theme.fontSizeMedium
            text: page.clock(camera.videoRecorder.duration)
        }

        // The other camera: the one facing the reader, for a picture of
        // themselves. Not while a video runs.
        IconButton {
            objectName: "flipButton"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: shutter.verticalCenter
            }
            enabled: !page.recording
            icon.source: "image://theme/icon-m-refresh"
            onClicked: camera.position = camera.position === Camera.FrontFace
                                         ? Camera.BackFace : Camera.FrontFace
        }
    }

    // The capture is being written: nothing to tap meanwhile.
    BusyIndicator {
        objectName: "writing"
        anchors.centerIn: parent
        running: page.done || camera.imageCapture.capturing
        size: BusyIndicatorSize.Large
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
