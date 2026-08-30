import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One search result, whatever kind it is. Laid out like a chat-list row so
 * the three groups read as one list: the same round avatar, the same
 * spacing, a second line for the detail.
 *
 * Titles and subtitles are names and message text the other end chose, so
 * both are pinned to PlainText -- see MessageDelegate.qml.
 */
Item {
    id: root

    property string title
    property string subtitle
    property string ownColor
    property string picturePath
    /// Unix seconds. 0 means the row has no time worth showing, which is
    /// what a contact result is: it has no moment attached to it.
    property int timestamp: 0

    height: Math.max(avatar.height, subtitleLabel.y + subtitleLabel.height)
            + 2 * Theme.paddingMedium

    /// Time for today, weekday for this week, date for older -- the same
    /// rule ChatListDelegate uses, so a chat reads the same in both lists.
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

    Avatar {
        id: avatar
        objectName: "resultAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.title
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    Label {
        id: timeLabel
        objectName: "resultTime"
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            top: titleLabel.top
        }
        visible: root.timestamp > 0
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.timeLabel(root.timestamp)
    }

    Label {
        id: titleLabel
        objectName: "resultTitle"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: root.width - x - Theme.horizontalPageMargin
               - (timeLabel.visible ? timeLabel.width + Theme.paddingMedium : 0)
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        text: root.title
    }

    Label {
        id: subtitleLabel
        objectName: "resultSubtitle"
        x: titleLabel.x
        y: titleLabel.y + titleLabel.height
        width: root.width - x - Theme.horizontalPageMargin
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.subtitle
    }
}
