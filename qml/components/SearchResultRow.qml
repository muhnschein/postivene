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
    // Nothing should reach past the edge even if a width binding is ever
    // wrong again; the labels below say what they do with the overflow.
    clip: true

    /// How long ago, in as few characters as will say it -- the same
    /// rule ChatListDelegate uses, so a chat reads the same in both
    /// lists.
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

    // Positioned off the row, never off the title. ChatListDelegate lays
    // its own time label out this way, and its name label takes its width
    // from this label's *x*. Anchoring this to the title while the title
    // subtracted this label's width closed a loop: neither width
    // resolved, so the title kept its natural one and a long chat name
    // ran off the right-hand edge.
    Label {
        id: timeLabel
        objectName: "resultTime"
        x: root.width - width - Theme.horizontalPageMargin
        y: Theme.paddingMedium
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
        width: (timeLabel.visible ? timeLabel.x : root.width - Theme.horizontalPageMargin)
               - x - Theme.paddingMedium
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
        // Two lines, then faded. A message that matched a search is
        // usually a sentence, and one line of it says too little to tell
        // two hits apart.
        wrapMode: Text.Wrap
        maximumLineCount: 2
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.subtitle
    }
}
