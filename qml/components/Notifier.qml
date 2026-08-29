import QtQuick 2.0
import Nemo.Notifications 1.0

/*
 * Raises a notification for a message that arrives while the reader is not
 * looking at the chat it landed in, and takes it back down when they are.
 *
 * One notification per chat, kept and reused rather than replaced, so two
 * chats talking at once do not overwrite each other in the notification
 * area -- the real Notification allocates an id on first publish and
 * `replacesId` reuses it.
 *
 * `ChatList` decides what counts as an arrival and never announces a muted
 * chat; this decides only whether the reader was there to see it.
 */
Item {
    id: notifier

    /// The chat on screen right now, 0 for none.
    property int viewingChatId: 0
    /// False while the app is in the background, where "on screen" is not
    /// the same as "seen".
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    /// chatId -> Notification.
    property var notes: ({})

    // Reading a chat is the answer to "have I seen this", so drop the
    // notification the moment the reader arrives.
    onViewingChatIdChanged: {
        if (viewingChatId !== 0 && appActive) {
            notifier.clear(viewingChatId)
        }
    }
    onAppActiveChanged: {
        if (appActive && viewingChatId !== 0) {
            notifier.clear(viewingChatId)
        }
    }

    property Component noteComponent: Component {
        Notification {
            category: "x-nemo.messaging.im"
        }
    }

    /// Announce a message, unless the reader is already looking at it.
    function arrived(chatId, chatName, preview) {
        if (appActive && chatId === viewingChatId) {
            return
        }
        var note = notifier.notes[chatId]
        if (!note) {
            note = noteComponent.createObject(notifier)
            if (!note) {
                return
            }
            notifier.notes[chatId] = note
        }
        // Both the banner and the entry that stays in the notification
        // area: the previewed pair is what appears over the top.
        note.summary = chatName
        note.body = preview
        note.previewSummary = chatName
        note.previewBody = preview
        note.publish()
    }

    /// Take down whatever is standing for this chat.
    function clear(chatId) {
        var note = notifier.notes[chatId]
        if (note) {
            note.close()
        }
    }

    /// How many chats are currently speaking for themselves. For tests.
    function publishedCount() {
        var total = 0
        for (var id in notifier.notes) {
            if (notifier.notes[id].isPublished) {
                total += 1
            }
        }
        return total
    }
}
