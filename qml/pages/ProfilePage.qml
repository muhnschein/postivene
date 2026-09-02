import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import Postivene 1.0

/*
 * One profile, as everyone else sees it and as this device holds it: the
 * picture, the name on every message, the line under it, the address it
 * writes from, whether the other end is told when something has been
 * read, and how the relay and the phone are doing by it. Reached from the
 * profile's row on the profiles page. The settings that belong to no
 * profile are in the system's Settings app instead
 * (qml/settings/GeneralSettingsPage.qml).
 *
 * The editable parts are core config keys rather than a record of their
 * own, so this page owns a Profile object over get_config/set_config
 * instead of a model. Nothing is confirmed: edits apply a moment after
 * typing stops and again on the way out, and the switch applies on the
 * tap. A settings page that needs saving is a settings page that loses
 * what was typed when someone swipes back.
 *
 * The connectivity and the mailbox quota follow parla's profile dialog
 * (github.com/trufae/parla): the core's own band for the connection, and
 * the sentence and the percentage it wrote on its own report.
 */
Page {
    id: page

    property int accountId

    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
        onError: page.errorMessage = message
        // The fields are only filled from the core, never re-filled from
        // it while someone is typing into them: a save reloads, and that
        // would otherwise reach in and reset the cursor.
        onLoaded_changed: {
            if (profile.loaded && !page.edited) {
                // Assigning to a field fires onTextChanged, which is the
                // same signal the reader typing produces. Without this
                // the load itself would count as an edit, and leaving
                // the page would write the profile back over itself.
                page.filling = true
                nameField.text = profile.display_name
                bioField.text = profile.status
                page.filling = false
            }
        }
    }

    Connections {
        target: core
        // The core says when the connection changes; the mailbox and
        // the storage are re-read with it.
        onCore_event: {
            if (kind === "ConnectivityChanged" && context_id === page.accountId) {
                profile.refresh_connectivity()
            }
        }
    }

    /// Someone has typed since the load. Guards the refill above.
    property bool edited: false
    /// The refill is writing to the fields, so the changes are not edits.
    property bool filling: false
    property string errorMessage: ""

    // A pause, not a keystroke: a round trip per letter would be four
    // calls to write "Ada".
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!profile.loaded || !page.edited) {
            return
        }
        page.edited = false
        profile.save(nameField.text, bioField.text)
    }

    function noteEdit() {
        if (profile.loaded && !page.filling) {
            page.edited = true
            autosave.restart()
        }
    }

    // Leaving is the other moment worth saving at: a back-swipe within
    // the pause above would otherwise drop what was typed.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
        }
    }

    // The gallery, pushed by URL and connected to, the way the
    // conversation attaches a photo: the Attach*Page files are the only
    // ones that name a `Sailfish.Pickers` type, so a type that is not
    // there costs this button rather than the page. That page also
    // ignores a cancelled pick, which this one used to hand to the core
    // as `undefined`.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // The core copies the file into its own blob directory,
                // so the picked one may go away afterwards.
                profile.set_picture(path)
            })
        }
    }

    /// The core's connectivity band in words. The bands are the core's
    /// (connectivity.rs); 0 is a profile not yet asked about.
    function connectionWords(state) {
        if (state >= 4000) {
            return qsTr("Connected")
        }
        if (state >= 3000) {
            return qsTr("Connected, sending and receiving")
        }
        if (state >= 2000) {
            return qsTr("Connecting")
        }
        if (state >= 1000) {
            return qsTr("Not connected")
        }
        return qsTr("Checking the connection")
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "refreshConnectivity"
                text: qsTr("Check connection")
                onClicked: profile.refresh_connectivity()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Profile")
            }

            // The picture is the control, not a preview of one: tapping
            // it opens the gallery, and the badge says so without a row
            // of its own.
            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "profileAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: nameField.text.length > 0
                             ? nameField.text
                             : profile.address
                    picturePath: profile.avatar_path
                }

                Rectangle {
                    id: editBadge
                    objectName: "editBadge"
                    anchors {
                        right: bigAvatar.right
                        bottom: bigAvatar.bottom
                    }
                    width: Theme.itemSizeExtraSmall
                    height: width
                    radius: width / 2
                    color: Theme.highlightBackgroundColor

                    Image {
                        anchors.centerIn: parent
                        source: "image://theme/icon-s-edit"
                    }
                }

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    onClicked: page.pickPicture()
                }
            }

            // Only offered when there is one to remove, and kept off the
            // picture itself: a tap that might delete is not a tap you
            // want under a finger reaching for the gallery.
            Button {
                objectName: "removePicture"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: profile.avatar_path.length > 0
                text: qsTr("Remove picture")
                onClicked: profile.clear_picture()
            }

            TextField {
                id: nameField
                objectName: "profileNameField"
                width: parent.width
                label: qsTr("Name")
                placeholderText: qsTr("Your name")
                onTextChanged: page.noteEdit()
            }

            TextField {
                id: bioField
                objectName: "profileBioField"
                width: parent.width
                label: qsTr("Bio")
                placeholderText: qsTr("A line about you")
                onTextChanged: page.noteEdit()
            }

            // The reader's own address: what the relay minted, and what
            // tells two profiles apart. Shown, not edited -- changing it
            // is a different transport, not a rename.
            DetailItem {
                objectName: "addressItem"
                label: qsTr("Address")
                value: profile.address
            }

            // The invite is how anyone gets in touch with this profile,
            // so the page about the profile leads to it.
            Button {
                objectName: "inviteButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Show invite code")
                onClicked: pageStack.push(Qt.resolvedUrl("InvitePage.qml"),
                                          { accountId: page.accountId })
            }

            TextSwitch {
                objectName: "readReceiptsSwitch"
                text: qsTr("Send read receipts")
                // The core's own words for mdns_enabled are "should be
                // sent and requested", so this is not one-way: with it
                // off nothing is asked for either, and read marks stop
                // coming back from the people who would have sent them.
                description: qsTr("Tells the people you write to when you have read their messages, and asks the same of them. With this off you send none and see none.")
                // Bound to the profile, not held here: the core is what
                // decides, and a switch that drifts from it is a lie. So
                // the tap must not flip it either -- Silica does that by
                // default, which detaches the binding on the first tap and
                // leaves a refused change looking like it took. The tap
                // asks the core for the other state, and the binding
                // shows whatever the core then says.
                automaticCheck: false
                checked: profile.read_receipts
                enabled: profile.loaded
                onClicked: profile.set_read_receipts(!checked)
            }

            SectionHeader {
                text: qsTr("Storage and connectivity")
            }

            Label {
                objectName: "connectivityLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                // A translated literal, chosen by the core's number.
                textFormat: Text.PlainText
                text: page.connectionWords(profile.connectivity)
            }

            // The mailbox on the relay, when the relay says how full it
            // is: the bar the core drew, and its own words beside it.
            ProgressBar {
                objectName: "quotaBar"
                width: parent.width
                visible: profile.quota_text.length > 0
                minimumValue: 0
                maximumValue: 100
                value: Math.min(100, profile.quota_percent)
                label: profile.quota_text
            }

            Label {
                objectName: "storageLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: profile.storage_bytes > 0
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                //: How much room the profile takes on the phone. %1 is a size such as "12.3 MB".
                text: qsTr("%1 on this phone").arg(Format.readableSize(profile.storage_bytes))
            }
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }

    Connections {
        target: profile
        // Quiet confirmation that something reached the core, since
        // nothing else on this page says so any more.
        onSaved: notice.show(qsTr("Saved"))
    }
}
