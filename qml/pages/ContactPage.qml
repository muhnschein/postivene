import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Who a one-to-one chat is with: their picture, their name, the line they
 * wrote about themselves, and whether the connection is checked and
 * encrypted. Reached by swiping left from the conversation, the way the
 * group page is from a group.
 *
 * Nothing here is an address. A reader of a chatmail app has no use for
 * one, and the core's own words for a contact are the name, the picture
 * and the status line.
 *
 * The name can be the reader's own for them: the field holds what was
 * given here and shows what they call themselves behind it, and leaving
 * it blank goes back to theirs -- the core's own rule for an empty name.
 * Applied a pause after typing stops and again on the way out, the way
 * the group page renames a group.
 *
 * The same ChatInfo as the group page: a one-to-one chat has one member,
 * the contact, so the model that lists a group's members lists them.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// The name the conversation page showed, until the core answers.
    property string chatName
    property string errorMessage: ""

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
        onSaved: notice.show(qsTr("Saved"))
    }

    Connections {
        target: core
        // A changed name or picture reaches here through the contact,
        // which the model reloads on.
        onCore_event: chat.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
            }
        }
    }

    /// The one member, lifted out of the row that reads it: the field
    /// sits outside the Repeater so a reload -- every save is one -- does
    /// not rebuild it under the reader's thumb.
    property int contactId: 0
    /// The name given here, empty when none was.
    property string givenName: ""
    /// The name they chose for themselves.
    property string ownName: ""

    /// Someone has typed since the load. Guards the refill below.
    property bool edited: false
    /// The refill is writing to the field, so the change is not an edit.
    property bool filling: false

    // Filled from the core, never re-filled from it while someone is
    // typing: every save reloads, and that would reset the cursor.
    onGivenNameChanged: {
        if (!page.edited) {
            page.filling = true
            nameField.text = page.givenName
            page.filling = false
        }
    }

    // A pause, not a keystroke: a round trip per letter would be three
    // calls to write "Ada".
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!chat.loaded || !page.edited || page.contactId === 0) {
            return
        }
        page.edited = false
        chat.rename_contact(page.contactId, nameField.text)
    }

    function noteEdit() {
        if (chat.loaded && !page.filling) {
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

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Contact")
            }

            // One row, the contact: a Repeater is how a model row is read
            // from QML, and the one here has exactly one.
            Repeater {
                model: chat.members

                delegate: Column {
                    objectName: "contactDetails"
                    width: column.width
                    spacing: Theme.paddingMedium

                    // Handed up to the page for the field below, which
                    // cannot live in here.
                    Binding { target: page; property: "contactId"; value: model.contact_id }
                    Binding { target: page; property: "givenName"; value: model.name }
                    Binding { target: page; property: "ownName"; value: model.auth_name }

                    Item {
                        width: parent.width
                        height: bigAvatar.height + 2 * Theme.paddingLarge

                        Avatar {
                            id: bigAvatar
                            objectName: "contactAvatar"
                            anchors.centerIn: parent
                            width: 2 * Theme.itemSizeExtraLarge
                            initial: model.display_name
                            ownColor: model.color
                            picturePath: model.avatar_path
                        }
                    }

                    // The name is the contact's own choice, so it is
                    // drawn as written.
                    Label {
                        objectName: "contactName"
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font.pixelSize: Theme.fontSizeLarge
                        color: Theme.highlightColor
                        text: model.display_name
                    }

                    // The same two facts the chat list marks a row with,
                    // said in words: whether the connection is encrypted,
                    // and whether it was checked in person.
                    Label {
                        objectName: "encryptionLabel"
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        font.pixelSize: Theme.fontSizeSmall
                        color: Theme.secondaryHighlightColor
                        // Translated literals, but pinned all the same:
                        // the binding reads the model to choose one.
                        textFormat: Text.PlainText
                        text: model.is_verified
                              ? qsTr("Verified: end-to-end encrypted, and checked in person")
                              : model.is_key_contact
                                ? qsTr("End-to-end encrypted")
                                : qsTr("Not encrypted: a plain email contact")
                    }

                    // What they wrote about themselves, when they did.
                    // Their words, so pinned to plain text.
                    Label {
                        objectName: "statusLabel"
                        visible: model.status.length > 0
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        color: Theme.primaryColor
                        text: model.status
                    }
                }
            }

            // What to call them here. Their own name stands in the empty
            // field, which is also what a blank field goes back to. A
            // field draws what it holds as text and nothing else, so it
            // needs no pinning to plain.
            TextField {
                id: nameField
                objectName: "contactNameField"
                width: parent.width
                label: qsTr("Name")
                placeholderText: page.ownName.length > 0 ? page.ownName : qsTr("Name")
                description: qsTr("Leave blank to use the name they chose")
                onTextChanged: page.noteEdit()
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
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
}
