import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0
import "pages"
import "cover"
import "components"

ApplicationWindow {
    id: appWindow

    /// The developer view's recorder: one for the app, since a recording
    /// has to outlive the page that starts it. SettingsPage hands it to
    /// DeveloperPage.
    property alias recorder: devRecorder

    DevRecorder {
        id: devRecorder
    }

    // The heartbeat. While a recording runs, an animation that never ends
    // keeps the scene graph presenting a frame every refresh, so a gap
    // between frames is a stall and not an idle screen -- and each of its
    // steps is a beat on the main thread, which tells a stall of that
    // thread from one of the render thread's.
    Item {
        id: heartbeat
        property real phase: 0
        onPhaseChanged: devRecorder.beat()

        NumberAnimation on phase {
            running: devRecorder.recording
            loops: Animation.Infinite
            from: 0
            to: 1
            duration: 1000
        }
    }

    initialPage: Component { WelcomePage {} }
    // Nothing is handled here any more: the cover's action was removed
    // along with the status label it was drawn on top of, and tapping
    // the cover already opens the app.
    cover: Component { CoverPage {} }

    // The one setting the core has to be told about: it applies to every
    // profile, and it follows the key as the settings page changes it.
    Binding {
        target: core
        property: "download_limit"
        value: Settings.downloadLimit
    }

    Component.onCompleted: {
        core.start(rpcServerPath)
    }
}
