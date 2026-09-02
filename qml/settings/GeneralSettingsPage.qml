import QtQuick 2.0
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0

/*
 * Postivene's page in the system's Settings app, under Apps.
 *
 * This file runs inside jolla-settings, not inside Postivene: the entry
 * in settings/harbour-postivene.json points the Settings app at it, and
 * the Settings app loads it in its own process. So nothing here can reach
 * the core or any component of ours -- only Silica and dconf, which is
 * where these three values live. The app reads the same keys through
 * qml/components/Settings.qml and follows them as they change.
 *
 * Everything on this page is a setting for the whole app rather than for
 * one profile; a profile's own settings are on its page inside the app.
 */
Page {
    id: page

    // What the app reads; the keys are the ones Settings.qml names.
    ConfigurationValue {
        id: markdownConfig
        objectName: "markdownConfig"
        key: "/apps/harbour-postivene/markdown_mode"
        defaultValue: 0
        // Silica writes currentIndex itself on a tap, which detaches the
        // binding, so the choice is put back from the key each time it
        // changes -- the arrangement DisappearingMessages uses.
        onValueChanged: markdownCombo.currentIndex = page.markdownIndex(value)
    }

    ConfigurationValue {
        id: cleanLinksConfig
        objectName: "cleanLinksConfig"
        key: "/apps/harbour-postivene/clean_links"
        defaultValue: false
    }

    ConfigurationValue {
        id: downloadConfig
        objectName: "downloadConfig"
        key: "/apps/harbour-postivene/download_limit"
        defaultValue: 1048576
        onValueChanged: downloadCombo.currentIndex = page.limitIndex(value)
    }

    /// The download limits offered, in bytes, as parla offers them. The
    /// first is the smallest the core accepts, which is as near to never
    /// as it goes; the last is no limit.
    readonly property var limits: [32768, 262144, 524288, 1048576, 2097152, 5242880, 0]

    function limitLabel(index) {
        switch (index) {
        case 0: return qsTr("Never")
        case 1: return qsTr("Up to 256 kB")
        case 2: return qsTr("Up to 512 kB")
        case 3: return qsTr("Up to 1 MB")
        case 4: return qsTr("Up to 2 MB")
        case 5: return qsTr("Up to 5 MB")
        default: return qsTr("Always")
        }
    }

    function limitIndex(bytes) {
        for (var i = 0; i < limits.length; i++) {
            if (limits[i] === bytes) {
                return i
            }
        }
        return 3
    }

    function markdownIndex(mode) {
        return mode >= 0 && mode <= 2 ? mode : 0
    }

    Component.onCompleted: {
        markdownCombo.currentIndex = page.markdownIndex(markdownConfig.value)
        downloadCombo.currentIndex = page.limitIndex(downloadConfig.value)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Postivene")
            }

            SectionHeader {
                text: qsTr("Messages")
            }

            ComboBox {
                id: markdownCombo
                objectName: "markdownCombo"
                width: parent.width
                label: qsTr("Markdown")
                description: qsTr("How a message written with *stars* and `backticks` is shown.")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "markdownOption0"
                        text: qsTr("Drawn: bold, italics, links")
                        onClicked: markdownConfig.value = 0
                    }
                    MenuItem {
                        objectName: "markdownOption1"
                        text: qsTr("Taken out: the words only")
                        onClicked: markdownConfig.value = 1
                    }
                    MenuItem {
                        objectName: "markdownOption2"
                        text: qsTr("As written")
                        onClicked: markdownConfig.value = 2
                    }
                }
            }

            ComboBox {
                id: downloadCombo
                objectName: "downloadCombo"
                width: parent.width
                label: qsTr("Auto-download attachments")
                description: qsTr("Bigger ones wait until you ask for them. Applies to every profile and to messages that arrive from now on.")

                menu: ContextMenu {
                    Repeater {
                        model: page.limits

                        MenuItem {
                            objectName: "downloadOption" + modelData
                            text: page.limitLabel(index)
                            onClicked: downloadConfig.value = modelData
                        }
                    }
                }
            }

            SectionHeader {
                text: qsTr("Links")
            }

            TextSwitch {
                objectName: "cleanLinksSwitch"
                text: qsTr("Remove tracking from links")
                description: qsTr("Known tracking parameters -- click ids, campaign tags, the sharer's account -- are taken out of the links in the messages you send. The rest of the link is left as it was.")
                // Bound to the key, not held here, so the switch cannot
                // drift from what the app will read.
                automaticCheck: false
                checked: cleanLinksConfig.value === true
                onClicked: cleanLinksConfig.value = !checked
            }
        }
    }
}
