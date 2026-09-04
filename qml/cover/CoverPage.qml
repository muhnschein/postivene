import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0
import "../components"

/*
 * What the cover has to say while the app is minimised: who is there,
 * and whether any of them has said something new.
 *
 * Three states. Nobody yet -- no chat but the one with oneself and the
 * core's own -- and the cover says so in a line. People, nothing new: a
 * grid of their avatars, in grey, the few repeated to fill it. Something
 * new: the same grid, dimmed, with the avatars of whoever wrote drawn
 * large over it and in their own colours, and the count in the corner.
 * "Delta" sits in the top left throughout: the cover is small, and the
 * name is what says which app this is.
 *
 * It keeps its own ChatList rather than reaching into the one the chat
 * list page owns, because a cover outlives any page -- the app can be
 * minimised from anywhere, including onboarding. That costs one extra
 * chat-list refetch per event, which is the price of the cover being
 * right whatever is on the stack. The people come out of it as one JSON
 * list (`cover_people`): the grid is laid out in a pass over them, which
 * a view over the rows could not repeat to fill.
 */
CoverBackground {
    id: cover

    property int accountId: 0

    ChatList {
        id: chats
        objectName: "coverChats"
        account_id: cover.accountId
        onRows_changed: cover.gather()
    }

    Connections {
        target: core
        onAccounts_refreshed: cover.accountId = first_configured_id
        onCore_event: chats.handle_event(context_id, kind, payload_json)
    }

    Component.onCompleted: {
        core.refresh_accounts()
        cover.gather()
    }

    readonly property bool hasUnread: chats.unread_total > 0

    /// Everyone, as the list hands them over.
    property var people: []
    /// Those with something new, first in the list's order.
    property var writers: []
    /// The rest, repeated to fill the grid's cells; empty when there is
    /// nobody to repeat.
    property var filler: []

    /// The grid's shape, from the room under the heading. Three across is
    /// what a cover's width takes at a size a face can still be told at.
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    readonly property int rows: cellSize > 0
                                ? Math.floor((cover.height - heading.height) / cellSize)
                                : 0

    /// Read the list again: who is there, who wrote, and what fills the
    /// grid. Called on every change to the rows and whenever the shape
    /// changes, so the filler is always the right length.
    function gather() {
        var everyone = []
        try {
            everyone = JSON.parse(chats.cover_people)
        } catch (err) {
            everyone = []
        }
        var loud = []
        var quiet = []
        for (var i = 0; i < everyone.length; i++) {
            if (everyone[i].unread_count > 0) {
                loud.push(everyone[i])
            } else {
                quiet.push(everyone[i])
            }
        }
        var cells = cover.columns * cover.rows
        var fill = []
        for (var cell = 0; quiet.length > 0 && cell < cells; cell++) {
            fill.push(quiet[cell % quiet.length])
        }
        cover.people = everyone
        cover.writers = loud
        cover.filler = fill
    }
    onRowsChanged: cover.gather()
    onColumnsChanged: cover.gather()

    // The name, and the number when there is one worth showing.
    Item {
        id: heading
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        height: brand.height + 2 * Theme.paddingMedium

        Label {
            id: brand
            objectName: "brand"
            anchors {
                left: parent.left
                leftMargin: Theme.paddingLarge
                verticalCenter: parent.verticalCenter
            }
            text: "Delta"
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.primaryColor
        }

        Label {
            id: unreadLabel
            objectName: "unreadTotal"
            anchors {
                right: parent.right
                rightMargin: Theme.paddingLarge
                verticalCenter: parent.verticalCenter
            }
            visible: cover.hasUnread
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
            text: chats.unread_total > 99 ? "99+" : chats.unread_total
        }
    }

    // Nobody yet: say so, in a line that wraps rather than runs off the
    // cover in a language where it is longer.
    Label {
        objectName: "emptyLabel"
        anchors {
            centerIn: parent
            verticalCenterOffset: heading.height / 2
        }
        width: parent.width - 2 * Theme.paddingLarge
        visible: cover.people.length === 0
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeSmall
        color: Theme.secondaryColor
        text: qsTr("No messages")
    }

    // Everyone, in grey, filling the room under the heading. Dimmed
    // further while someone is drawn over it.
    Grid {
        id: grid
        objectName: "avatarGrid"
        anchors {
            top: heading.bottom
            left: parent.left
        }
        columns: cover.columns
        opacity: cover.writers.length > 0 ? 0.35 : 0.7

        Repeater {
            model: cover.filler

            Avatar {
                objectName: "gridCell"
                width: cover.cellSize
                initial: modelData.name
                ownColor: modelData.color
                picturePath: modelData.avatar_path
                monochrome: true
            }
        }
    }

    // Whoever wrote, large and in colour, across the middle. Three fit;
    // past that the count in the corner says how much more there is.
    Row {
        id: writersRow
        objectName: "writersRow"
        anchors {
            centerIn: parent
            verticalCenterOffset: heading.height / 2
        }
        spacing: Theme.paddingMedium

        Repeater {
            model: cover.writers.slice(0, 3)

            Avatar {
                objectName: "writerAvatar"
                width: cover.writers.length > 2 ? Theme.itemSizeSmall
                                                : Theme.itemSizeMedium
                initial: modelData.name
                ownColor: modelData.color
                picturePath: modelData.avatar_path
            }
        }
    }
}
