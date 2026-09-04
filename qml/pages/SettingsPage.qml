import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The settings that belong to no profile: how a message is drawn, what
 * goes out with a link, how much of an attachment arrives unasked, and
 * how much a notification gives away. Reached from the chat list's
 * pull-down. A profile's own settings --
 * picture, name, address, read receipts, what the relay says -- are on
 * the profile's page, reached from its row on the profiles page.
 *
 * The values live in dconf, behind the `Settings` singleton every page
 * reads (qml/components/Settings.qml); this page writes the same object,
 * so a change here reaches every open page without either side being
 * told. They were briefly a page in the system's Settings app instead,
 * which never showed up on a device: the entry that puts a page there is
 * outside the paths Harbour allows, and it turned out to need more than
 * that entry to appear at all.
 *
 * Nothing here needs saving: each control writes its setting on the tap.
 */
Page {
    id: page

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

    function notificationIndex(detail) {
        return detail >= 0 && detail <= 2 ? detail : 0
    }

    /// When the title was tapped, oldest first, kept to the last three
    /// seconds.
    property var titleTaps: []

    /// A tap on the title at `now`, in milliseconds. Ten within three
    /// seconds open the developer view (DeveloperPage.qml); nothing else
    /// leads there. Apart from the MouseArea so a test can hand in the
    /// clock.
    function noteTitleTap(now) {
        var recent = []
        for (var i = 0; i < page.titleTaps.length; i++) {
            if (now - page.titleTaps[i] <= 3000) {
                recent.push(page.titleTaps[i])
            }
        }
        recent.push(now)
        if (recent.length >= 10) {
            page.titleTaps = []
            page.openDeveloperView()
        } else {
            page.titleTaps = recent
        }
    }

    function openDeveloperView() {
        // The recorder is the root window's, so a recording outlives the
        // page. A page loaded on its own, as a test does, has no window.
        pageStack.push(Qt.resolvedUrl("DeveloperPage.qml"), {
            recorder: typeof appWindow !== "undefined" ? appWindow.recorder : null
        })
    }

    /// Put each choice back to what the setting holds. Silica writes
    /// currentIndex itself on a tap, which detaches a binding, so the
    /// choice is put back from the setting each time it changes -- the
    /// arrangement DisappearingMessages uses.
    function refresh() {
        markdownCombo.currentIndex = page.markdownIndex(Settings.markdownMode)
        downloadCombo.currentIndex = page.limitIndex(Settings.downloadLimit)
        notificationCombo.currentIndex =
            page.notificationIndex(Settings.notificationDetail)
    }

    Connections {
        target: Settings
        onMarkdownModeChanged: page.refresh()
        onDownloadLimitChanged: page.refresh()
        onNotificationDetailChanged: page.refresh()
    }

    Component.onCompleted: page.refresh()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Settings")

                // Ten taps on the title within three seconds open the
                // developer view. Over the header only, and a drag
                // started here still reaches the flickable, which takes
                // the gesture over once it is one.
                MouseArea {
                    objectName: "titleTaps"
                    anchors.fill: parent
                    onClicked: page.noteTitleTap(Date.now())
                }
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
                        onClicked: Settings.markdownMode = 0
                    }
                    MenuItem {
                        objectName: "markdownOption1"
                        text: qsTr("Taken out: the words only")
                        onClicked: Settings.markdownMode = 1
                    }
                    MenuItem {
                        objectName: "markdownOption2"
                        text: qsTr("As written")
                        onClicked: Settings.markdownMode = 2
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
                            onClicked: Settings.downloadLimit = modelData
                        }
                    }
                }
            }

            SectionHeader {
                text: qsTr("Notifications")
            }

            // What a notification says is what the lock screen shows to
            // whoever is looking at it, so the reader chooses how much.
            ComboBox {
                id: notificationCombo
                objectName: "notificationCombo"
                width: parent.width
                label: qsTr("A new message shows")
                description: qsTr("On the lock screen and in the notification area. The chat it is from opens on a tap either way.")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "notificationOption0"
                        text: qsTr("Who wrote, and what")
                        onClicked: Settings.notificationDetail = 0
                    }
                    MenuItem {
                        objectName: "notificationOption1"
                        text: qsTr("Who wrote")
                        onClicked: Settings.notificationDetail = 1
                    }
                    MenuItem {
                        objectName: "notificationOption2"
                        text: qsTr("Only that something arrived")
                        onClicked: Settings.notificationDetail = 2
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
                // Bound to the setting, not held here, so the switch cannot
                // drift from what the app will read.
                automaticCheck: false
                checked: Settings.cleanLinks === true
                onClicked: Settings.cleanLinks = !checked
            }
        }
    }
}
