import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * One conversation. The messages come from a ChatMessages instance owned by
 * this page, so a second open conversation cannot reset this one's model.
 */
Page {
    id: page

    property int accountId
    property int chatId
    property string chatName

    ChatMessages {
        id: messages
        objectName: "messages"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
        onSent: textField.text = ""
    }

    property string errorMessage: ""

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        // The model ignores events for other accounts and chats itself.
        onCore_event: messages.handle_event(context_id, kind, payload_json)
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                messages.reload()
            } else if (core.status === "stopped") {
                page.errorMessage = qsTr("Lost the connection to the Delta Chat core. Restart Postivene.")
            }
        }
        onCore_error: page.errorMessage = message
    }

    SilicaListView {
        id: listView
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        model: messages.rows

        header: PageHeader {
            title: page.chatName
        }

        delegate: ListItem {
            objectName: "messageRow"
            // Sized by its text, not fixed: a device message runs to a dozen
            // wrapped lines, and a fixed row height makes them overlap each
            // other and the header.
            contentHeight: Math.max(Theme.itemSizeSmall,
                                    messageLabel.implicitHeight + 2 * Theme.paddingMedium)

            Label {
                id: messageLabel
                objectName: "messageLabel"
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                // A mail icon marks messages that were not encrypted and
                // signed. Outgoing messages get a DC_STATE_* suffix.
                text: (model.show_padlock ? "" : "✉ ") + model.text
                      + (model.is_outgoing ? " " + stateMark(model.state) : "")
                wrapMode: Text.Wrap
                horizontalAlignment: model.is_outgoing ? Text.AlignRight : Text.AlignLeft
                color: model.is_outgoing ? Theme.highlightColor : Theme.primaryColor

                function stateMark(state) {
                    if (state === 28) return "✓✓"
                    if (state === 26) return "✓"
                    if (state === 24) return "✗"
                    if (state === 20) return "…"
                    return ""
                }
            }
        }

        ViewPlaceholder {
            enabled: listView.count === 0
            text: qsTr("No messages yet")
        }
    }

    ErrorBanner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
        text: page.errorMessage
        // A dead core does not fix itself, so that one stays put.
        timeout: core.status === "stopped" ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    Row {
        id: inputRow
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        spacing: Theme.paddingSmall

        TextField {
            id: textField
            objectName: "messageField"
            width: parent.width - sendButton.width
            placeholderText: qsTr("Message")
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: page.sendCurrentText()
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            icon.source: "image://theme/icon-m-send"
            onClicked: page.sendCurrentText()
        }
    }

    function sendCurrentText() {
        if (textField.text.length > 0) {
            page.errorMessage = ""
            messages.send(textField.text)
        }
    }
}
