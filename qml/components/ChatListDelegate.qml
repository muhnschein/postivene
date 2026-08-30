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

    /// How long ago, in as few characters as will say it.
    ///
    /// "10 min" reads faster than "14:32" when the answer wanted is
    /// "recently", and it does not need the reader to know what time it
    /// is now. Past a week the elapsed count stops meaning anything, so
    /// it goes back to a date.
    function timeLabel(seconds) {
        if (seconds <= 0) {
            return ""
        }
        var when = new Date(seconds * 1000)
        var elapsed = (new Date()).getTime() - when.getTime()
        if (elapsed < 60000) {
            return qsTr("now")
        }
        if (elapsed < 3600000) {
            return qsTr("%1 min").arg(Math.floor(elapsed / 60000))
        }
        if (elapsed < 86400000) {
            return qsTr("%1 h").arg(Math.floor(elapsed / 3600000))
        }
        if (elapsed < 7 * 86400000) {
            var days = Math.floor(elapsed / 86400000)
            return days === 1 ? qsTr("1 day") : qsTr("%1 days").arg(days)
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

    // Muted, when it was, and pinned -- all three on the right, so the
    // name is a name rather than a name with luggage.
    Row {
        id: marks
        x: root.width - width - Theme.horizontalPageMargin
        y: Theme.paddingMedium
        spacing: Theme.paddingSmall

        Label {
            objectName: "muteMark"
            visible: root.isMuted
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: "🔇"
        }

        Label {
            id: timeLabelItem
            objectName: "timeLabel"
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: root.timeLabel(root.lastUpdated)
        }

        Label {
            objectName: "pinMark"
            visible: root.isPinned
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: "📌"
        }
    }

    Label {
        id: nameLabel
        objectName: "nameLabel"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: marks.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        // A mail icon marks a chat that cannot be encrypted to. Pinned
        // and muted are shown on the right instead: they are about where
        // the chat sits and how it behaves, not about what it is called.
        text: (root.isEncrypted ? "" : "✉ ") + root.chatName
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
