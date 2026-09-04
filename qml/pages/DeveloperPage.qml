import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The developer view. Hidden: ten taps on the Settings page's title within
 * three seconds, and nothing else leads here.
 *
 * What it does is make the phone the profiler. A recording writes, once a
 * second, what the app and the core weigh and do -- proportional memory,
 * CPU by thread, frames presented and the longest gap between them -- to
 * a directory under Documents, with the marks typed here while something
 * is being reproduced, a full memory map on demand, the kernel facts
 * docs/SECURITY.md left to a device to answer, and a script for the one
 * thing the app cannot do from inside its sandbox, which is trace its own
 * syscalls. docs/BUILDING.md says how to read what it writes.
 *
 * English only, on purpose: this is for whoever is profiling the app,
 * and forty catalogs of "smaps" would be forty catalogs of nothing. The
 * recording outlives this page -- the point is to go elsewhere in the app
 * while it runs -- so the recorder belongs to the root window and is
 * handed in.
 */
Page {
    id: page

    /// The app's one DevRecorder, from the root window. Null when the page
    /// is loaded without one, which only a test does.
    property QtObject recorder: null

    readonly property bool ready: page.recorder !== null
    readonly property bool recording: page.ready && page.recorder.recording

    Component.onCompleted: {
        if (page.ready) {
            page.recorder.probe_system()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: "Developer"
            }

            SectionHeader {
                text: "Recording"
            }

            Label {
                objectName: "recordingState"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                text: page.recording
                      ? "Recording to " + page.recorder.output_dir
                      : "Not recording. Start, then use the app: open a chat "
                        + "full of pictures, play a GIF, open one full screen. "
                        + "Come back here to mark what you did, or to stop."
            }

            Button {
                objectName: "recordButton"
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: page.ready
                text: page.recording ? "Stop recording" : "Start recording"
                onClicked: {
                    if (page.recording) {
                        page.recorder.stop()
                    } else {
                        page.recorder.start()
                    }
                }
            }

            Label {
                objectName: "summary"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                visible: page.recording
                text: page.ready ? page.recorder.summary : ""
            }

            SectionHeader {
                text: "Marks"
            }

            TextField {
                id: markField
                objectName: "markField"
                width: parent.width
                label: "Mark"
                placeholderText: "What you are about to do"
                enabled: page.recording
            }

            Row {
                x: Theme.horizontalPageMargin
                spacing: Theme.paddingMedium

                Button {
                    objectName: "markButton"
                    enabled: page.recording
                    text: "Mark"
                    onClicked: {
                        page.recorder.mark(markField.text)
                        markField.text = ""
                    }
                }

                Button {
                    objectName: "snapshotButton"
                    enabled: page.recording
                    text: "Memory snapshot"
                    onClicked: page.recorder.snapshot()
                }
            }

            Label {
                objectName: "status"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: page.ready ? page.recorder.status : "No recorder: loaded without the root window"
            }

            SectionHeader {
                text: "Syscalls"
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                text: "The app cannot trace itself inside its sandbox. Each "
                      + "recording writes strace.sh with both process ids in "
                      + "it; over SSH, as root, `sh <recording>/strace.sh 60` "
                      + "traces sixty seconds of both and lists the distinct "
                      + "syscalls of each."
            }

            SectionHeader {
                text: "System"
            }

            Label {
                objectName: "systemReport"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                text: page.ready ? page.recorder.system_report : ""
            }
        }
    }
}
