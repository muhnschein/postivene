import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0

/*
 * What the cover has to say while the app is minimised: how much is
 * waiting.
 *
 * The core's status and a shortcut into the chat list both used to sit
 * along the bottom, drawn over each other -- a CoverAction and a Label
 * claim the same strip. Both are gone rather than one moved: the status
 * is the app's business and not the reader's, and a cover that opens the
 * app is what tapping the cover already does.
 *
 * It keeps its own ChatList rather than reaching into the one the chat
 * list page owns, because a cover outlives any page -- the app can be
 * minimised from anywhere, including onboarding. That costs one extra
 * chat-list refetch per event, which is the price of the cover being
 * right whatever is on the stack.
 */
CoverBackground {
    id: cover

    property int accountId: 0

    ChatList {
        id: chats
        objectName: "coverChats"
        account_id: cover.accountId
    }

    Connections {
        target: core
        onAccounts_refreshed: cover.accountId = first_configured_id
        onCore_event: chats.handle_event(context_id, kind, payload_json)
    }

    Component.onCompleted: core.refresh_accounts()

    readonly property bool hasUnread: chats.unread_total > 0

    Label {
        id: title
        anchors {
            top: parent.top
            topMargin: Theme.paddingLarge
            horizontalCenter: parent.horizontalCenter
        }
        text: "Postivene"
        font.pixelSize: Theme.fontSizeLarge
    }

    // The number, when there is one worth showing.
    Label {
        id: unreadLabel
        objectName: "unreadTotal"
        anchors.centerIn: parent
        visible: cover.hasUnread
        font.pixelSize: Theme.fontSizeHuge
        color: Theme.highlightColor
        text: chats.unread_total > 99 ? "99+" : chats.unread_total
    }

    // Nothing waiting: say so rather than leave a blank cover.
    Label {
        objectName: "quietLabel"
        anchors.centerIn: parent
        visible: !cover.hasUnread
        font.pixelSize: Theme.fontSizeSmall
        color: Theme.secondaryColor
        text: qsTr("No new messages")
    }

}
