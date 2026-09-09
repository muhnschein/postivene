import QtQuick 2.0

/*
 * The wait before something is destroyed, owned by the list rather than
 * by the row.
 *
 * Silica has this already -- `ListItem.remorseAction` -- and puts the
 * countdown in an item parented to the row it was asked for on. For
 * deleting things *out of* a list that is the one place it cannot go: a
 * delete is exactly what destroys rows, so the first one to land takes
 * the countdown of everything else asked for with it. Deleting a run of
 * messages lost most of them, which is how this was found, and the chat
 * list, the profiles list and a group's members were all open to the
 * same thing -- any reload or reorder destroys rows, and a message
 * arriving reorders the chat list.
 *
 * So the countdown lives here, beside the list rather than inside it,
 * and one wait covers everything asked for while it runs: asking for
 * several is one go, and one wait is one thing to change your mind
 * about.
 *
 * Whoever holds one of these draws a waiting row however that row
 * should look -- `pending(id)` says which -- and calls `spare(id)` when
 * the reader taps it to take it back. A page on its way out calls
 * `flush()`, for the reason ConversationPage writes its draft there:
 * leaving is exactly when a timer has not fired yet, and somebody who
 * asked for a thing to go and then left asked for it to go.
 *
 * A `QtObject` rather than an `Item`: it draws nothing, and an Item
 * declared inside a Column would take a turn in the layout.
 */
QtObject {
    id: root

    /// What to do to each one, once the wait is up.
    signal remove(int id)

    /// How long the wait is, in milliseconds. Silica's own remorse waits
    /// five seconds; four, because this one starts again with every id
    /// added to it, and a run should not keep the reader waiting much
    /// longer than one does. A property because a test turns it down
    /// rather than sitting through it.
    property int delay: 4000

    /// The ids waiting, as a set.
    ///
    /// Replaced rather than changed in place -- a binding does not re-run
    /// when the contents of an object it read change, only when the
    /// property is assigned. Which is what makes `pending()` usable in a
    /// row's bindings at all.
    property var ids: ({})

    /// Whether this one is on its way out.
    function pending(id) {
        return root.ids[id] === true
    }

    /// When the wait now running is up, as a millisecond clock reading.
    /// 0 when nothing is waiting.
    ///
    /// Kept so a row rebuilt mid-wait -- scrolled out of the view and
    /// back -- can put the platform's countdown up again with the time
    /// that is actually left on it, rather than a fresh one or none.
    property real dueAt: 0

    /// How much of the wait is left for this one, in milliseconds. 0 if
    /// it is not waiting at all.
    function remaining(id) {
        if (!root.pending(id)) {
            return 0
        }
        return Math.max(1, root.dueAt - Date.now())
    }

    /// Ask for one to go, once the reader has had their moment.
    function ask(id) {
        var next = root.copied()
        next[id] = true
        root.ids = next
        root.dueAt = Date.now() + root.delay
        root.countdown.restart()
    }

    /// The reader said they did not mean it, about one of them. The rest
    /// keep going.
    function spare(id) {
        var next = root.copied()
        delete next[id]
        root.ids = next
        if (Object.keys(next).length === 0) {
            root.countdown.stop()
            root.dueAt = 0
        }
    }

    /// Everything still waiting goes now.
    function flush() {
        var going = root.ids
        root.ids = ({})
        root.countdown.stop()
        root.dueAt = 0
        // Cleared before any of them is acted on: whatever answers
        // `remove` may come straight back here, and must not find a
        // list this is still working through.
        var waiting = Object.keys(going)
        for (var i = 0; i < waiting.length; i++) {
            root.remove(parseInt(waiting[i], 10))
        }
    }

    /// The set as it stands, to be changed and assigned back.
    function copied() {
        var next = {}
        for (var key in root.ids) {
            next[key] = root.ids[key]
        }
        return next
    }

    property Timer countdown: Timer {
        objectName: "removalCountdown"
        interval: root.delay
        onTriggered: root.flush()
    }
}
