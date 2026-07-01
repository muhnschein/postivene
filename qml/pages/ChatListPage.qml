import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    property int accountId

    Component.onCompleted: core.refresh_chat_list(accountId)

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: core.chat_list

        header: PageHeader {
            title: qsTr("Chats")
        }

        PullDownMenu {
            MenuItem {
                text: qsTr("Refresh")
                onClicked: core.refresh_chat_list(accountId)
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
                    text: model.name
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
            enabled: listView.count === 0
            text: qsTr("No chats yet")
        }
    }
}
