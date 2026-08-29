import QtQuick 2.0
Item {
    id: root

    property int contentHeight: 80
    property bool down: false
    property bool menuOpen: false
    property var menu
    signal clicked()

    // Silica counts down before acting, and runs the action anyway if it is
    // destroyed while the countdown is still going (RemorseItem's own
    // `Component.onDestruction`). Both halves matter: a row that goes away
    // mid-countdown still deletes something, and what it deletes is
    // whatever the callback can still resolve by then.
    property var _pendingAction: null

    Timer {
        id: remorseCountdown
        interval: 50
        onTriggered: {
            var action = root._pendingAction
            root._pendingAction = null
            if (action) action()
        }
    }

    function remorseAction(text, action) {
        root._pendingAction = action
        remorseCountdown.restart()
    }

    Component.onDestruction: {
        if (remorseCountdown.running && root._pendingAction) {
            root._pendingAction()
        }
    }

    width: parent ? parent.width : 540
    height: contentHeight
}
