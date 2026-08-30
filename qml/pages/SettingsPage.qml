import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0
import "../components"
import Postivene 1.0

/*
 * The profile as everyone else sees it: the name on every message, the
 * line under it, and the picture.
 *
 * All three are core config keys, so this page owns a Profile rather than
 * a model. Edits apply when the page is left as well as from the pulley:
 * a back-swipe is how a Silica page is normally finished with, and
 * dropping what was typed because of it is the kind of loss that is never
 * noticed until it matters.
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
                statusField.text = profile.status
                page.filling = false
            }
        }
    }

    /// Someone has typed since the load. Guards the refill above.
    property bool edited: false
    /// The refill is writing to the fields, so the changes are not edits.
    property bool filling: false
    property string errorMessage: ""

    function applyEdits() {
        if (!profile.loaded || !page.edited) {
            return
        }
        page.edited = false
        profile.save(nameField.text, statusField.text)
    }

    // Applied on the way out as well as from the pulley.
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

        PullDownMenu {
            MenuItem {
                objectName: "savePicture"
                text: qsTr("Change picture")
                onClicked: pageStack.push(picker)
            }
            MenuItem {
                objectName: "removePicture"
                visible: profile.avatar_path.length > 0
                text: qsTr("Remove picture")
                onClicked: profile.clear_picture()
            }
            MenuItem {
                objectName: "saveProfile"
                text: qsTr("Save")
                onClicked: page.applyEdits()
            }
        }

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: qsTr("Settings")
            }

            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "profileAvatar"
                    anchors.centerIn: parent
                    width: Theme.itemSizeExtraLarge
                    initial: nameField.text.length > 0
                             ? nameField.text
                             : profile.address
                    picturePath: profile.avatar_path
                }
            }

            TextField {
                id: nameField
                objectName: "profileNameField"
                width: parent.width
                label: qsTr("Name")
                placeholderText: qsTr("Your name")
                onTextChanged: {
                    if (profile.loaded && !page.filling) {
                        page.edited = true
                    }
                }
            }

            TextField {
                id: statusField
                objectName: "profileStatusField"
                width: parent.width
                label: qsTr("Signature")
                placeholderText: qsTr("Sent with Postivene")
                onTextChanged: {
                    if (profile.loaded && !page.filling) {
                        page.edited = true
                    }
                }
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
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
