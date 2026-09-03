import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"
import "components"

ApplicationWindow {
    id: appWindow

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
