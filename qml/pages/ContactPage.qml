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
