import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Which profile the chat list is showing.
 *
 * The core keeps every configured account open at once, so switching is a
 * matter of pointing the chat list at a different one rather than starting
 * anything up. Picking the account already shown does nothing.
 */
Page {
    id: page

    /// The account the chat list is currently on.
    property int currentAccountId: 0

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: core.account_list

        header: PageHeader {
            title: qsTr("Accounts")
        }

        delegate: ListItem {
            objectName: "accountRow"
            contentHeight: body.height

            ContactRow {
                id: body
                width: parent.width
                // An account has no colour of its own from the core, so
                // the initial sits on the theme's highlight.
                displayName: model.display_name.length > 0
                             ? model.display_name : model.addr
                address: model.addr
                isKeyContact: true
            }

            // The one being shown, marked the way a chosen group member is.
            Label {
                objectName: "currentMark"
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: body.verticalCenter
                }
                visible: model.account_id === page.currentAccountId
                text: "✓"
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeLarge
            }

            onClicked: {
                if (model.account_id !== page.currentAccountId) {
                    // Replaced rather than pushed: the chat list for the
                    // old account is not somewhere to go "back" to.
                    pageStack.replace(Qt.resolvedUrl("ChatListPage.qml"),
                                      { accountId: model.account_id })
                } else {
                    pageStack.pop()
                }
            }
        }

        ViewPlaceholder {
            enabled: listView.count === 0
            text: qsTr("No accounts")
        }
    }
}
