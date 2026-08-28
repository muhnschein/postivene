import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0

/*
 * Connect with someone by invite, which is how Delta Chat contacts are
 * normally added: an address alone cannot be encrypted to
 * (docs/ONBOARDING.md).
 *
 * Both directions are text. Camera scanning is the same payload read
 * optically and needs a scanner on the device; until then a link pasted
 * from a message, a browser or another app does the same job.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""
    property string myInvite: ""
    property bool joining: false

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: {
            page.joining = false
            page.errorMessage = message
        }
        onInvite_ready: page.myInvite = link
        onChat_ready: {
            page.joining = false
            pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chat_id,
                chatName: qsTr("Chat")
            })
        }
    }

    Component.onCompleted: contacts.fetch_invite()

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.fetch_invite()
            }
        }
    }

    function follow() {
        if (linkField.text.length === 0) {
            return
        }
        page.errorMessage = ""
        page.joining = true
        contacts.join_by_invite(linkField.text)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("Invite")
            }

            SectionHeader {
                text: qsTr("Follow an invite")
            }

            TextField {
                id: linkField
                objectName: "linkField"
                width: parent.width
                label: qsTr("Invite link")
                placeholderText: "https://i.delta.chat/..."
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            }

            Button {
                objectName: "followButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Connect")
                enabled: !page.joining && linkField.text.length > 0
                onClicked: page.follow()
            }

            Label {
                objectName: "errorLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.errorMessage.length > 0
                color: Theme.errorColor
                text: page.errorMessage
            }

            SectionHeader {
                text: qsTr("Your invite")
            }

            Label {
                objectName: "myInviteLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: page.myInvite.length > 0 ? page.myInvite
                                               : qsTr("Fetching...")
            }

            Button {
                objectName: "copyButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Copy Invite Link")
                enabled: page.myInvite.length > 0
                onClicked: Clipboard.text = page.myInvite
            }
        }
    }
}
