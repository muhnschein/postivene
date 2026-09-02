import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Connect with someone by invite, which is how Delta Chat contacts are
 * normally added: an address alone cannot be encrypted to
 * (docs/PROJECT.md).
 *
 * Both directions are text under the pictures: the code on this page is
 * the invite link drawn, and a code scanned is read back into a link and
 * followed the way a pasted one is. Pasting stays, for a link that
 * arrived in a message or a browser.
 *
 * The code is an Image of a file the shim writes rather than a Canvas:
 * a Canvas is drawn into the window's GL context, which the platform
 * takes away from an app in the background, and it came back blank; an
 * Image is reloaded from its file.
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
            // Back from the scanner, if it is what asked, to where the
            // message is.
            if (pageStack.currentPage !== page) {
                pageStack.pop(page)
            }
        }
        onInvite_ready: page.myInvite = link
        onChat_ready: {
            page.joining = false
            // Above the page that opened this one: the scanner may still
            // be on top of this page, and goes too.
            pageStack.replaceAbove(pageStack.previousPage(page),
                                   Qt.resolvedUrl("ConversationPage.qml"), {
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

    // The scanner, pushed by URL and connected to, the way the pickers
    // are: ScanPage is the only file that names a Camera. It stays up
    // while the invite is followed, and the chat replaces both pages.
    function scan() {
        var scanner = pageStack.push(Qt.resolvedUrl("ScanPage.qml"))
        if (scanner) {
            scanner.scanned.connect(function(text) {
                page.errorMessage = ""
                page.joining = true
                contacts.join_by_invite(text)
            })
        }
    }

    // The invite as a picture, for a phone held up to this one.
    QrCode {
        id: qr
        objectName: "qr"
        text: page.myInvite
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

            Button {
                objectName: "scanButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Scan QR code")
                enabled: !page.joining
                onClicked: page.scan()
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }

            SectionHeader {
                text: qsTr("Your invite")
            }

            // Drawn on white whatever the ambience: a code is read by
            // contrast, and an ambience can have little. Not smoothed:
            // the modules stay square edges rather than blur into each
            // other.
            Image {
                objectName: "inviteQr"
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.floor(parent.width * 0.7)
                height: width
                visible: qr.size > 0
                source: qr.image
                smooth: false
                cache: false
                fillMode: Image.PreserveAspectFit
            }

            Label {
                objectName: "myInviteLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
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
