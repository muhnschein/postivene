import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Name a group and pick its members. Groups are created encrypted, which is
 * what the reference client's "New Group" does.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""
    // True from tapping create until the core answers, so a second tap
    // cannot make a second group.
    property bool creating: false
    // Contact ids the user has ticked.
    property var members: []

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: {
            page.creating = false
            page.errorMessage = message
        }
        onChat_ready: {
            page.creating = false
            pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chat_id,
                chatName: nameField.text
            })
        }
    }

    function toggle(contactId) {
        var next = []
        var found = false
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] === contactId) {
                found = true
            } else {
                next.push(page.members[i])
            }
        }
        if (!found) {
            next.push(contactId)
        }
        page.members = next
    }

    function isMember(contactId) {
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] === contactId) {
                return true
            }
        }
        return false
    }

    function createGroup() {
        if (nameField.text.length === 0) {
            page.errorMessage = qsTr("Please name the group")
            return
        }
        page.errorMessage = ""
        page.creating = true
        contacts.create_group(nameField.text, page.members)
    }

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    // Outside the list for the reason ChatListPage documents: a field in
    // a view's `header` lives inside the flickable and its id does not
    // resolve from the page. Here that was not merely a dead search --
    // `nameField.text` is what names the group and what enables the
    // Create button, so both were reading an undefined name.
    // One flickable for the whole page, owning the pulley.
    //
    // A PullDownMenu draws at the top of the flickable that owns
    // it, and the list starts below the search field -- so a pulley
    // on the list opened below the field, or inside it. It does not
    // scroll: the field has to stay out of a view whose contents
    // change on every keystroke, which is what took the keyboard
    // away mid-search.
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        contentHeight: height

        PullDownMenu {
            MenuItem {
                objectName: "createButton"
                text: qsTr("Create Group")
                enabled: !page.creating && nameField.text.length > 0
                onClicked: page.createGroup()
            }
        }

        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                title: qsTr("New Group")
            }

            TextField {
                id: nameField
                objectName: "nameField"
                width: parent.width
                label: qsTr("Group name")
                placeholderText: label
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }

        SilicaListView {
            id: listView
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            model: contacts.rows

            delegate: ListItem {
                objectName: "memberRow"
                contentHeight: body.height
                // A group here is encrypted, and the core takes only
                // key-contacts into one -- picking anyone else builds a group
                // they cannot be added to, which fails halfway through.
                enabled: model.is_key_contact

                ContactRow {
                    id: body
                    width: parent.width
                    displayName: model.display_name
                    address: model.address
                    ownColor: model.color
                    picturePath: model.avatar_path
                    isKeyContact: model.is_key_contact
                    isVerified: model.is_verified
                    // Greyed where the core would refuse them, highlighted
                    // where they are already in.
                    opacity: model.is_key_contact ? 1.0 : 0.4
                }

                // The tick sits on the avatar rather than in the name, so the
                // row reads the same as every other contact row.
                Label {
                    objectName: "memberMark"
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: body.verticalCenter
                    }
                    visible: page.isMember(model.contact_id)
                    text: "✓"
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeLarge
                }

                onClicked: if (model.is_key_contact) page.toggle(model.contact_id)
            }

            ViewPlaceholder {
                enabled: contacts.count === 0
                text: qsTr("No contacts to add yet")
            }
        }
    }
}
