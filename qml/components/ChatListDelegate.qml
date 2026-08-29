import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One chat-list row. The name and the preview are whatever the other end
 * sent, so both are pinned to PlainText -- see MessageDelegate.qml.
 *
 * Its own component so it can be loaded and measured on
 * its own, and laid out with bindings rather than a positioner -- see
 * MessageDelegate.qml.
 */
Item {
    id: root

    property string chatName
    property string preview
    // Who wrote the last message; worth showing where there are several.
    property string previewSender
    property int unreadCount: 0
    // Unix seconds.
    property double lastUpdated: 0
    property bool isEncrypted: true
    property bool isPinned: false
    property bool isMuted: false
    property bool isContactRequest: false
    // The core's per-chat colour, for the avatar.
    property string chatColor
    property string avatarPath
    // DC_STATE_* of the last message; 20 and up means we sent it.
    property int summaryState: 0

    readonly property bool summaryIsOurs: summaryState >= 20

    height: Math.max(avatar.height, previewLabel.y + previewLabel.height)
            + 2 * Theme.paddingMedium

    /// Time for today, weekday for this week, date for older.
    function timeLabel(seconds) {
        if (seconds <= 0) {
            return ""
        }
        var when = new Date(seconds * 1000)
        var now = new Date()
        if (when.toDateString() === now.toDateString()) {
            return Qt.formatTime(when, "hh:mm")
        }
        if (now.getTime() - when.getTime() < 6 * 86400000) {
            return Qt.formatDate(when, "ddd")
        }
        return Qt.formatDate(when, Qt.DefaultLocaleShortDate)
    }

    function stateMark(state) {
        if (state === 28) return "✓✓"
        if (state === 26) return "✓"
        if (state === 24) return "✗"
        if (state === 20) return "…"
        return ""
    }

    // The chat's picture, or its colour with an initial on it.
    Avatar {
        id: avatar
        objectName: "avatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.chatName
        ownColor: root.chatColor
        picturePath: root.avatarPath
    }

    Label {
        id: timeLabelItem
        objectName: "timeLabel"
        x: root.width - width - Theme.horizontalPageMargin
        y: Theme.paddingMedium
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.timeLabel(root.lastUpdated)
    }

    Label {
        id: nameLabel
        objectName: "nameLabel"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: timeLabelItem.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        // A mail icon marks a chat that cannot be encrypted to; the other
        // two say how the chat is filed.
        text: (root.isEncrypted ? "" : "✉ ") + root.chatName
              + (root.isPinned ? " 📌" : "") + (root.isMuted ? " 🔇" : "")
    }

    Label {
        id: previewLabel
        objectName: "previewLabel"
        x: nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: badge.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        // Ours carries its delivery mark; someone else's is named when the
        // core names them, which it does where it matters.
        text: (root.summaryIsOurs ? root.stateMark(root.summaryState) + " " : "")
              + (root.previewSender.length > 0 && !root.summaryIsOurs
                 ? root.previewSender + ": " : "")
              + root.preview
    }

    // Unread count. A muted chat still counts, quietly.
    Rectangle {
        id: badge
        objectName: "unreadBadge"
        visible: root.unreadCount > 0
        width: visible ? Math.max(height, badgeLabel.implicitWidth + Theme.paddingMedium) : 0
        height: visible ? badgeLabel.implicitHeight + Theme.paddingSmall : 0
        x: root.width - width - Theme.horizontalPageMargin
        y: previewLabel.y
        radius: height / 2
        color: root.isMuted ? Theme.rgba(Theme.secondaryColor, 0.4)
                            : Theme.highlightColor

        Label {
            id: badgeLabel
            objectName: "unreadLabel"
            anchors.centerIn: parent
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.primaryColor
            text: root.unreadCount > 99 ? "99+" : root.unreadCount
        }
    }
}
