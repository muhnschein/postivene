import QtQuick 2.0

/*
 * Stands in for Silica's RemorseItem: the platform's countdown drawn
 * over the thing about to be destroyed, with a tap to call it off.
 *
 * Written from the real file rather than guessed at, because the last
 * guess about this type cost a release. What it does on `execute`:
 *
 *     parent = item.parent
 *     _item = item
 *     anchors.fill: _item
 *     PropertyChanges { target: _item; opacity: 0.0 }
 *
 * -- so it becomes a sibling of the item it covers, takes that item's
 * geometry, and *fades* the item rather than hiding it. The distinction
 * matters: hiding takes the item's children's `visible` with it, and
 * anything measuring `visible ? implicitHeight : 0` then collapses.
 *
 * It also runs the callback rather than dropping it when the countdown
 * is cut short -- by the item being destroyed, or by the page it is on
 * deactivating. The app does not rely on either (the deletion itself
 * belongs to components/PendingRemoval.qml, and this draws it), but the
 * stub does both so a test sees what a phone would.
 *
 * The countdown is 50ms rather than the real five seconds: a test says
 * what it wants and reads the answer on the next tick.
 */
Item {
    id: root

    property string text
    property string cancelText: "Tap to cancel"
    signal canceled()
    signal triggered()

    /// What it is drawn over, and whether it is drawn at all. A test
    /// reads these to say "the platform's countdown is up on this row".
    property Item _item: null
    readonly property bool active: root._item !== null

    property var _callback: null

    function execute(item, title, callback, timeout) {
        root.text = title
        root._callback = callback
        root._item = item
        // A sibling of what it covers, filling it, with the item faded
        // rather than hidden -- see the note above.
        root.parent = item.parent
        root.anchors.fill = item
        item.opacity = 0.0
        countdown.interval = timeout === undefined ? 50 : Math.min(timeout, 50)
        countdown.restart()
    }

    function cancel() {
        if (!root.active) {
            return
        }
        countdown.stop()
        root._forget()
        root.canceled()
    }

    /// The tap that a reader gives it, which is a cancel.
    function tap() {
        root.cancel()
    }

    function _forget() {
        if (root._item) {
            root._item.opacity = 1.0
        }
        root._item = null
        root._callback = null
    }

    function _run() {
        var callback = root._callback
        countdown.stop()
        root._forget()
        if (callback) {
            callback()
        }
        root.triggered()
    }

    Timer {
        id: countdown
        interval: 50
        onTriggered: root._run()
    }

    // Cut short rather than cancelled: the real one acts rather than
    // forgetting, and so does this.
    Component.onDestruction: {
        if (countdown.running) {
            root._run()
        }
    }
}
