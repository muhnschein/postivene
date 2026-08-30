import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0
import "../components"
import Postivene 1.0

/*
 * The profile as everyone else sees it: the picture, the name on every
 * message, the line under it, and whether the other end is told when
 * something has been read.
 *
 * All of it is core config keys rather than a record of its own, so this
 * page owns a Profile object over get_config/set_config instead of a
 * model. Nothing is confirmed: edits apply a moment after typing stops
 * and again on the way out, and the two switches apply on the tap. A
 * settings page that needs saving is a settings page that loses what was
 * typed when someone swipes back.
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

    Component {
        id: picker
        ImagePickerPage {
            onSelectedContentPropertiesChanged: {
                // The core copies the file into its own blob directory,
                // so the picked one may go away afterwards.
                profile.set_picture(selectedContentProperties.filePath)
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Settings")
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
                    onClicked: pageStack.push(picker)
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

            TextSwitch {
                objectName: "readReceiptsSwitch"
                text: qsTr("Send read receipts")
                description: qsTr("Lets the people you write to see when you have read their messages. Turning it off does not stop you seeing theirs.")
                // Bound to the profile, not held here: the core is what
                // decides, and a switch that drifts from it is a lie.
                checked: profile.read_receipts
                enabled: profile.loaded
                onClicked: profile.set_read_receipts(checked)
            }

            // Not editable: changing the address is setting up another
            // profile, not renaming this one.
            DetailItem {
                objectName: "profileAddress"
                label: qsTr("Address")
                value: profile.address
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
