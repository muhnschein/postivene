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

    // The scanner, pushed by URL and connected to, the way the pickers
    // are: ScanPage is the only file that names a Camera.
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
            // contrast, and an ambience can have little.
            Canvas {
                id: qrCanvas
                objectName: "inviteQr"
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.floor(parent.width * 0.7)
                height: width
                visible: qr.size > 0
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.fillStyle = "white"
                    ctx.fillRect(0, 0, width, height)
                    var n = qr.size
                    if (n === 0) {
                        return
                    }
                    // Four modules of quiet zone, as the standard asks.
                    var quiet = 4
                    var cell = width / (n + 2 * quiet)
                    var rows = qr.modules.split("\n")
                    ctx.fillStyle = "black"
                    for (var y = 0; y < n; y++) {
                        for (var x = 0; x < n; x++) {
                            if (rows[y].charAt(x) === "1") {
                                ctx.fillRect((x + quiet) * cell, (y + quiet) * cell,
                                             Math.ceil(cell), Math.ceil(cell))
                            }
                        }
                    }
                }
            }

            Connections {
                target: qr
                onModules_changed: qrCanvas.requestPaint()
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
