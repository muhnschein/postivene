import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Point the camera at a QR code -- or, from the pull-down, type or paste
 * the link the code would carry. What either says is handed back on
 * `scanned` and the page waits; what to do with it is the caller's -- an
 * invite is followed, a provider payload creates a profile -- and the
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
 * Once a code is read the page stays, with the camera off and a busy
 * indicator on, until the caller takes it down: Silica drops any stack
 * operation asked for while a transition is running, so a page that
 * popped itself here would have the caller's own navigation -- opening
 * the chat the invite led to -- land during the pop and be lost. The
 * typed link is a panel on this page rather than a dialog for the same
 * reason: a dialog's own pop would be the transition the caller's
 * navigation lands in.
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

    /// A code was read, or a link typed, and this is its text.
    signal scanned(string text)

    property string errorMessage: ""
    /// Set once a code is read, so a second frame in flight cannot
    /// report the same code twice.
    property bool done: false
    /// The link panel is open. The camera keeps running behind it: a
    /// reader who opened the panel and then found the code can still
    /// hold the phone up to it.
    property bool typing: false

    /// What was read or typed, once it is worth acting on.
    function found(text) {
        if (page.done) {
            return
        }
        page.done = true
        camera.stop()
        page.scanned(text)
    }

    /// Open the link panel, with the clipboard's contents in it when
    /// they look like an invite: pasting is what the panel is for, and a
    /// link copied a moment ago is the likely reason it was opened.
    function typeLink() {
        page.typing = true
        var clip = Clipboard.text
        if (linkField.text.length === 0 && clip.length > 0 && page.looksLikeInvite(clip)) {
            linkField.text = clip
        }
    }

    /// Whether some text is one of the things a code carries, so the
    /// clipboard is not pasted into the field when it holds a shopping
    /// list. Only the prefixes: what the payload is is the core's call.
    function looksLikeInvite(text) {
        var trimmed = text.trim().toLowerCase()
        return trimmed.indexOf("https://i.delta.chat/") === 0
            || trimmed.indexOf("openpgp4fpr:") === 0
            || trimmed.indexOf("dcaccount:") === 0
            || trimmed.indexOf("dclogin:") === 0
    }

    function useLink() {
        var text = linkField.text.trim()
        if (text.length === 0) {
            return
        }
        page.found(text)
    }

    QrScanner {
        id: scanner
        objectName: "scanner"
        onFound: page.found(text)
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

    // A flickable that does not scroll, for the pull-down's sake: the
    // page is the viewfinder, and the way to a typed link hangs off its
    // top the way every other way to something does.
    SilicaFlickable {
        anchors.fill: parent
        contentHeight: height

        PullDownMenu {
            MenuItem {
                objectName: "typeLinkItem"
                text: qsTr("Enter invite link")
                onClicked: page.typeLink()
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

        PageHeader {
            title: qsTr("Scan QR code")
        }

        // The code was read and the caller is acting on it.
        BusyIndicator {
            objectName: "acting"
            anchors.centerIn: parent
            running: page.done
            size: BusyIndicatorSize.Large
        }

        // The other way in: the link the code would carry, typed or
        // pasted. Over the viewfinder rather than instead of it.
        Column {
            id: linkPanel
            objectName: "linkPanel"
            visible: page.typing && !page.done
            anchors {
                left: parent.left
                right: parent.right
                bottom: hint.top
            }
            spacing: Theme.paddingSmall

            Rectangle {
                width: parent.width
                height: linkField.height + followButton.height + 2 * Theme.paddingMedium
                color: Theme.rgba(Theme.highlightDimmerColor, 0.8)

                TextField {
                    id: linkField
                    objectName: "linkField"
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        topMargin: Theme.paddingMedium
                    }
                    label: qsTr("Invite link")
                    placeholderText: "https://i.delta.chat/..."
                    inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                }

                Button {
                    id: followButton
                    objectName: "followButton"
                    anchors {
                        horizontalCenter: parent.horizontalCenter
                        top: linkField.bottom
                    }
                    text: qsTr("Connect")
                    enabled: linkField.text.trim().length > 0
                    onClicked: page.useLink()
                }
            }
        }

        Label {
            id: hint
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
            visible: !page.done
            text: page.typing
                  ? qsTr("Or point the camera at the code")
                  : qsTr("Point the camera at an invite or a chatmail server code. Pull down to enter a link instead.")
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
}
