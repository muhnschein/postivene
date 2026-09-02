import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Which profile the chat list is showing.
 *
 * The core keeps every configured account open at once, so switching is a
 * matter of pointing the chat list at a different one rather than starting
 * anything up. Picking the profile already shown does nothing.
 */
Page {
    id: page

    /// The profile the chat list is currently on.
    property int currentAccountId: 0

    /// True once a deletion has been asked for, so the empty list that
    /// follows is read as "the last profile is gone" rather than as the
    /// list simply not having loaded yet.
    property bool deleting: false

    Connections {
        target: core
        // Deleting the last profile leaves the app with nothing to show,
        // so it goes back to where a first profile is made. Replacing the
        // whole stack, not pushing: there is no chat list to return to.
        onAccounts_refreshed: {
            if (page.deleting && configured_count === 0) {
                page.deleting = false
                // The empty properties are passed rather than left
                // out: Silica's own replaceAbove takes them, and the
                // two-argument form errors out under a stack that
                // declares all three -- which is what the test harness
                // does, so the branch was never actually run there.
                pageStack.replaceAbove(null,
                                       Qt.resolvedUrl("WelcomePage.qml"),
                                       {})
            }
        }
        onAccount_error: page.errorMessage = message
    }

    property string errorMessage: ""

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: core.account_list

        // Another profile is made the way the first was: the welcome
        // page's own flow, which replaces the stack with the new
        // profile's chat list once the core has it.
        PullDownMenu {
            MenuItem {
                objectName: "addProfileMenuItem"
                text: qsTr("Add profile")
                onClicked: pageStack.push(Qt.resolvedUrl("CreateProfilePage.qml"), {})
            }
        }

        header: PageHeader {
            title: qsTr("Profiles")
        }

        delegate: ListItem {
            id: profileDelegate
            objectName: "profileRow"
            contentHeight: body.height

            menu: ContextMenu {
                MenuItem {
                    objectName: "deleteProfileItem"
                    text: qsTr("Delete profile")
                    // The id is taken now rather than read inside the
                    // callback: the row is destroyed when the list
                    // reloads, and Silica runs a remorse action on that
                    // destruction, by which point `model` resolves to
                    // nothing. Same reason the chat list hoists its own.
                    onClicked: {
                        var doomed = model.account_id
                        profileDelegate.remorseAction(qsTr("Deleting profile"),
                                                      function() {
                                                          page.deleting = true
                                                          core.remove_account(doomed)
                                                      })
                    }
                }
            }

            ContactRow {
                id: body
                width: parent.width
                // An account has no colour of its own from the core, so
                // the initial sits on the theme's highlight.
                displayName: model.display_name.length > 0
                             ? model.display_name : model.addr
                address: model.addr
                // The reader's own, and what tells two profiles apart.
                showAddress: true
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
                    // The whole stack, not just this page. `replace`
                    // swapped out the accounts page and left the previous
                    // account's chat list underneath it -- one swipe back
                    // into the profile just left. A null target replaces
                    // everything, which is what the onboarding pages do
                    // when they hand over to the chat list.
                    pageStack.replaceAbove(null,
                                           Qt.resolvedUrl("ChatListPage.qml"),
                                           { accountId: model.account_id })
                } else {
                    pageStack.pop()
                }
            }
        }

        ViewPlaceholder {
            enabled: listView.count === 0
            text: qsTr("No profiles")
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
