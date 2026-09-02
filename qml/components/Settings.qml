pragma Singleton
import QtQuick 2.0
import Nemo.Configuration 1.0

/*
 * The settings that belong to no profile: how a message is drawn, what
 * goes out with a link, and how much of an attachment arrives unasked.
 *
 * They live in dconf under the app's own path, because the page that
 * changes them is not in this app. It sits in the system's Settings app
 * -- qml/settings/GeneralSettingsPage.qml, listed by
 * settings/harbour-postivene.json -- which runs in another process and
 * can reach nothing of ours but these keys. Each value here follows its
 * key, so a change made there reaches every page that reads this without
 * either side being told.
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

    // The keys, named once here and once in the settings page: the two
    // files cannot share a definition, so tests/qml_syntax.rs holds them
    // to the same strings.
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
}
