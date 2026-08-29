import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"

ApplicationWindow {
    id: appWindow

    initialPage: Component { WelcomePage {} }
    // The cover cannot reach pageStack from its own file; here it can.
    // Taking the reader to the chat list is the one thing a cover action
    // can do that tapping the cover does not already do better -- a tap
    // returns to whatever page was left open, which after a long
    // conversation is rarely where they want to be.
    cover: Component {
        CoverPage {
            onShowChats: {
                appWindow.activate()
                while (pageStack.depth > 1) {
                    pageStack.pop(undefined, PageStackAction.Immediate)
                }
            }
        }
    }

    Component.onCompleted: {
        core.start(rpcServerPath)
    }
}
