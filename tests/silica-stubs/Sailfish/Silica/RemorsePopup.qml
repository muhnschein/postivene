import QtQuick 2.0

// Silica's page-level countdown, the ListItem's remorseAction for an
// action that is nobody's row. Counts down briefly and then runs the
// action, as the real one does when nobody cancels.
Item {
    property var _pendingAction: null

    Timer {
        id: countdown
        interval: 50
        onTriggered: {
            var action = _pendingAction
            _pendingAction = null
            if (action) action()
        }
    }

    function execute(text, action) {
        _pendingAction = action
        countdown.restart()
    }
}
