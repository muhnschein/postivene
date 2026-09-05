import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0
import "../components"

/*
 * What the cover has to say while the app is minimised: who is there,
 * and whether any of them has said something new.
 *
 * The heading is laid out as the platform's own covers lay theirs out
 * -- the calendar's, say: the name top left with a line under it, and
 * the number top right, large. Under it, everyone's avatar in a
 * staggered grid, in grey, the few repeated to fill it; whoever has
 * written is drawn in their own colours where they stand in the grid.
 * Nobody yet -- no chat but the one with oneself and the core's own --
 * and the cover says so in a line instead.
 *
 * Every profile counts: one ChatList per configured profile, so the
 * number is every unread message on the phone and the grid is everyone
 * the reader talks to, whichever profile they are under. The lists are
 * the cover's own rather than the chat list page's, because a cover
 * outlives any page -- the app can be minimised from anywhere, including
 * onboarding. The people come out of each list as one JSON list
 * (`cover_people`): the grid is laid out in a pass over them, which a
 * view over the rows could not repeat to fill.
 */
CoverBackground {
    id: cover

    // One list per profile. A Repeater's delegates have to be Items,
    // so each list sits in an empty one.
    Repeater {
        id: lists
        model: core.account_list

        delegate: Item {
            visible: false
            width: 0
            height: 0
            property alias chats: chats

            ChatList {
                id: chats
                objectName: "coverChats"
                account_id: model.is_configured ? model.account_id : 0
                onRows_changed: cover.gather()
            }
        }
    }

    Connections {
        target: core
        onAccounts_refreshed: cover.gather()
        onCore_event: {
            for (var i = 0; i < lists.count; i++) {
                lists.itemAt(i).chats.handle_event(context_id, kind, payload_json)
            }
        }
    }

    Component.onCompleted: {
        core.refresh_accounts()
        cover.gather()
    }

    /// Everyone, across every profile, in each list's order.
    property var people: []
    /// Unread messages across every profile.
    property int unreadTotal: 0
    /// What the grid draws: `{person, row, col, loud}` per cell, the
    /// people repeated to fill it and `loud` on the first cell of anyone
    /// with something new.
    property var cells: []

    /// The grid's shape: three across, with every other row shifted half
    /// a cell and holding one more, cut off at both edges -- so the rows
    /// nest, and the grid reads as a field of faces rather than a table.
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    readonly property int rowStep: Math.max(1, Math.round(cover.cellSize * 0.9))
    readonly property int rows: cover.cellSize > 0
                                ? Math.ceil(grid.height / cover.rowStep)
                                : 0

    /// Read the lists again: who is there, how much is unread, and what
    /// fills the grid. Called on every change to any list and whenever
    /// the shape changes, so the cells are always the right number.
    function gather() {
        var everyone = []
        var total = 0
        for (var i = 0; i < lists.count; i++) {
            var chats = lists.itemAt(i).chats
            total += chats.unread_total
            var some = []
            try {
                some = JSON.parse(chats.cover_people)
            } catch (err) {
                some = []
            }
            for (var j = 0; j < some.length; j++) {
                // Keyed by profile too: two profiles can hold the same
                // chat id.
                some[j].key = i + ":" + some[j].chat_id
                everyone.push(some[j])
            }
        }
        var made = []
        var lit = {}
        var next = 0
        for (var row = 0; row < cover.rows && everyone.length > 0; row++) {
            var across = row % 2 === 0 ? cover.columns + 1 : cover.columns
            for (var col = 0; col < across; col++) {
                var person = everyone[next % everyone.length]
                next++
                var loud = person.unread_count > 0 && !lit[person.key]
                if (loud) {
                    lit[person.key] = true
                }
                made.push({ person: person, row: row, col: col, loud: loud })
            }
        }
        cover.people = everyone
        cover.unreadTotal = total
        cover.cells = made
    }
    onRowsChanged: cover.gather()

    // The name and what it is, top left; the number top right, always
    // -- a zero says as much as a count.
    Column {
        id: heading
        anchors {
            top: parent.top
            left: parent.left
            right: unreadLabel.left
            margins: Theme.paddingLarge
            rightMargin: Theme.paddingMedium
        }
        // The two lines are one heading: closer than their own leading
        // would put them.
        spacing: -Theme.paddingSmall

        // Wrapped rather than faded: a cover is narrow, and the word
        // for "Messages" is a long one in several of the languages the
        // app speaks. The grid starts under whatever this comes to.
        Label {
            id: brand
            objectName: "brand"
            width: parent.width
            text: "Delta"
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeMedium
            wrapMode: Text.Wrap
        }

        Label {
            objectName: "subtitle"
            width: parent.width
            text: qsTr("Messages")
            color: Theme.secondaryHighlightColor
            font.pixelSize: Theme.fontSizeExtraSmall
            wrapMode: Text.Wrap
        }
    }

    Label {
        id: unreadLabel
        objectName: "unreadTotal"
        anchors {
            top: parent.top
            right: parent.right
            topMargin: Theme.paddingMedium
            rightMargin: Theme.paddingLarge
        }
        font.pixelSize: Theme.fontSizeHuge
        color: Theme.primaryColor
        text: cover.unreadTotal > 99 ? "99+" : cover.unreadTotal
    }

    // Nobody yet: say so, in a line that wraps rather than runs off the
    // cover in a language where it is longer.
    Label {
        objectName: "emptyLabel"
        anchors.centerIn: grid
        width: parent.width - 2 * Theme.paddingLarge
        visible: cover.people.length === 0
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeSmall
        color: Theme.secondaryColor
        text: qsTr("No messages")
    }

    // Everyone, filling the room under the heading. The shifted rows run
    // past both edges by half a cell, which the clip takes care of.
    Item {
        id: grid
        objectName: "avatarGrid"
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            topMargin: Theme.paddingLarge
        }
        clip: true

        Repeater {
            model: cover.cells

            Avatar {
                objectName: "gridCell"
                x: modelData.col * cover.cellSize
                   - (modelData.row % 2 === 0 ? cover.cellSize / 2 : 0)
                y: modelData.row * cover.rowStep
                width: cover.cellSize - Theme.paddingSmall
                initial: modelData.person.name
                ownColor: modelData.person.color
                picturePath: modelData.person.avatar_path
                monochrome: !modelData.loud
                opacity: modelData.loud ? 1.0 : 0.6
            }
        }
    }
}
