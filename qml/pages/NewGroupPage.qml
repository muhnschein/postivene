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
    // Contact ids the user has ticked.
    property var members: []

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
        onChat_ready: pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: nameField.text
        })
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

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: contacts.rows

        header: Column {
            width: page.width

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

            Button {
                objectName: "createButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Create Group")
                enabled: nameField.text.length > 0
                onClicked: page.createGroup()
            }
        }

        delegate: ListItem {
            objectName: "memberRow"
            contentHeight: Theme.itemSizeSmall

            Label {
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                textFormat: Text.PlainText
                text: (page.isMember(model.contact_id) ? "✓ " : "") + model.display_name
                color: page.isMember(model.contact_id) ? Theme.highlightColor
                                                       : Theme.primaryColor
                truncationMode: TruncationMode.Fade
            }

            onClicked: page.toggle(model.contact_id)
        }

        ViewPlaceholder {
            enabled: contacts.count === 0
            text: qsTr("No contacts to add yet")
        }
    }
}
