import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

Page {
    id: page

    property int accountId

    ChatList {
        id: chats
        objectName: "chats"
        account_id: page.accountId
        onError: page.errorMessage = message
    }

    property string errorMessage: ""
    readonly property string coreStoppedMessage:
        qsTr("Lost the connection to the Delta Chat core. Restart Postivene.")

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml. The model ignores
        // events for other accounts itself.
        onCore_event: chats.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                chats.reload()
            }
        }
        // Failures that used to reach no one.
        onCore_error: page.errorMessage = message
        onIo_started: {
            if (!success) {
                page.errorMessage = error
            }
        }
    }

    SilicaListView {
        id: listView
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        // Delegates draw outside the list's own box otherwise, and the
        // banner below is translucent.
        clip: true
        model: chats.rows

        header: PageHeader {
            title: qsTr("Chats")
        }

        PullDownMenu {
            MenuItem {
                objectName: "newChatMenuItem"
                text: qsTr("New chat")
                onClicked: pageStack.push(Qt.resolvedUrl("NewChatPage.qml"),
                                          { accountId: page.accountId })
            }
        }

        delegate: ListItem {
            id: delegateRoot
            contentHeight: Theme.itemSizeMedium

            Column {
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                spacing: Theme.paddingSmall

                Label {
                    width: parent.width
                    // Unencrypted chats get a letter mark; encrypted is the
                    // unmarked normal case.
                    text: model.is_encrypted ? model.name : "✉ " + model.name
                    truncationMode: TruncationMode.Fade
                }

                Label {
                    width: parent.width
                    text: model.preview
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
            }

            onClicked: pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: model.chat_id,
                chatName: model.name
            })
        }

        ViewPlaceholder {
            enabled: chats.count === 0
            text: qsTr("No chats yet")
            hintText: qsTr("Pull down to start one")
        }
    }

    ErrorBanner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        // A dead core outranks whatever failed before it, and a page opened
        // after it died never saw the transition -- so read the status
        // rather than waiting for it to change.
        text: core.status === "stopped" ? page.coreStoppedMessage : page.errorMessage
        // That one does not fix itself, so it stays put.
        timeout: core.status === "stopped" ? 0 : 8
        onDismissed: page.errorMessage = ""
    }
}
