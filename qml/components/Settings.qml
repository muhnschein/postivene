pragma Singleton
import QtQuick 2.0
import Nemo.Configuration 1.0

/*
 * The settings that belong to no profile: how a message is drawn, what
 * goes out with a link, how much of an attachment arrives unasked, and
 * how much a notification gives away.
 *
 * They live in dconf under the app's own path, and every page reads
 * them through this one object: the settings page (qml/pages/
 * SettingsPage.qml) writes it, and each value follows its key, so a
 * change made there reaches every open page without either side being
 * told. dconf rather than a property of the window, so the values
 * outlive the app the way a setting should.
 *
 * A singleton rather than a property handed down page by page: a message
 * row three components deep wants the same answer as the page that
 * sends, and the row is loaded on its own in a test.
 */
QtObject {
    /// 0 draws Markdown, 1 takes its markers out, 2 shows it as written.
    property alias markdownMode: markdownValue.value
    /// Take known tracking parameters out of links before sending.
    property alias cleanLinks: cleanLinksValue.value
    /// Attachments bigger than this many bytes wait to be asked for; 0
    /// fetches everything. The core's own `download_limit`, applied to
    /// every profile.
    property alias downloadLimit: downloadLimitValue.value
    /// How much a notification says: 0 who wrote and what, 1 who wrote,
    /// 2 only that something arrived. What the lock screen shows to
    /// whoever is looking at it.
    property alias notificationDetail: notificationDetailValue.value

    // The keys, named here and nowhere else: tests/qml_syntax.rs holds
    // every other file to reading them through this object.
    property ConfigurationValue markdownConfig: ConfigurationValue {
        id: markdownValue
        key: "/apps/harbour-postivene/markdown_mode"
        defaultValue: 0
    }

    property ConfigurationValue cleanLinksConfig: ConfigurationValue {
        id: cleanLinksValue
        key: "/apps/harbour-postivene/clean_links"
        defaultValue: false
    }

    property ConfigurationValue downloadLimitConfig: ConfigurationValue {
        id: downloadLimitValue
        key: "/apps/harbour-postivene/download_limit"
        // One megabyte, as parla defaults it: a photo arrives, a video
        // waits to be asked for.
        defaultValue: 1048576
    }

    property ConfigurationValue notificationDetailConfig: ConfigurationValue {
        id: notificationDetailValue
        key: "/apps/harbour-postivene/notification_detail"
        defaultValue: 0
    }
}
