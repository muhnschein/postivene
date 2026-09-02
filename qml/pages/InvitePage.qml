import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * This profile's invite, which is how anyone gets in touch with it: an
 * address alone cannot be encrypted to (docs/PROJECT.md), and a chatmail
 * address is nothing anybody types.
 *
 * The code is the invite link drawn, for a phone held up to this one;
 * the link itself is there to copy into anything else. Someone else's
 * code is read by the scanner, which the pull-down reaches, and that
 * page is also where a link can be typed -- so nothing is pasted here.
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
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "scanButton"
                text: qsTr("Scan QR code")
                enabled: !page.joining
                onClicked: page.scan()
            }
        }

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("Invite")
            }

            Label {
                objectName: "intro"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Let someone scan this code, or send them the link. To add someone from their code, pull down.")
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

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }
    }
}
