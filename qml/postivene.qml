import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"

ApplicationWindow {
    id: appWindow

    initialPage: Component { SetupPage {} }
    cover: Component { CoverPage {} }

    Component.onCompleted: {
        core.start(rpcServerPath)
    }
}
