import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

Page {
    id: page

    property int accountId
    /// Shows the archived chats instead of the ordinary ones. The two
    /// lists are disjoint, so this is a mode rather than a filter.
    property bool archived: false
    /// How many configured accounts there are, from the core. Decides
    /// whether switching is worth offering at all.
    property int accountCount: 0

    // Not bound straight to the field: a round trip per keystroke asks the
    // core four times to type "anna", and only the last answer is wanted.
    // Same interval as the contact search, for the same reason.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: {
            // The archived list is a mode over one kind of thing, so it
            // filters itself. The ordinary list searches everything and
            // shows the three kinds grouped, the way the reference
            // clients do.
            if (page.archived) {
                chats.query = searchField.text
            } else {
                searchModel.query = searchField.text
            }
        }
    }

    /// Who the chat being made from a contact result is with, so the
    /// conversation that opens is not headed by an empty title.
    property string pendingChatName: ""

    /// Results are showing instead of the chat list. Read from the field
    /// rather than from the model, so the swap happens on the keystroke
    /// and not a debounce later.
    readonly property bool searching:
        !page.archived && searchField.text.trim().length > 0

    SearchResults {
        id: searchModel
        objectName: "search"
        account_id: page.accountId
        onError: page.errorMessage = message
        // A contact result has no chat until it is tapped.
        onChat_ready: {
            notifier.viewingChatId = chat_id
            pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chat_id,
                chatName: page.pendingChatName
            })
        }
    }

    ChatList {
        id: chats
        objectName: "chats"
        account_id: page.accountId
        archived: page.archived
        onError: page.errorMessage = message
    }

    // Sits here rather than in the root window because this is where the
    // chat list already lives: the model decides what counts as an
    // arrival, and the page knows which chat the reader walked into.
    Notifier {
        id: notifier
        objectName: "notifier"
    }

    Connections {
        target: chats
        onMessage_arrived: notifier.arrived(chat_id, chat_name, preview)
    }

    // Back on the list means no chat is being read.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            notifier.viewingChatId = 0
        }
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
                // A search typed before the core was up found nothing and
                // had nothing to answer with; this is when it can.
                searchModel.reload()
            }
        }
        // Failures that used to reach no one.
        onAccounts_refreshed: page.accountCount = configured_count
        onCore_error: page.errorMessage = message
        onIo_started: {
            if (!success) {
                page.errorMessage = error
            }
        }
    }

    Component.onCompleted: core.refresh_accounts()

    // Outside the list, not in its `header`. A header item lives inside
    // the view's flickable, and both plausible explanations for the
    // reported focus loss run through that: Silica hides the input panel
    // when a flickable's content moves, and the narrowing list moves it on
    // every keystroke pause; and a view is a focus scope, which can hand
    // focus to a delegate as rows come and go. Anchored out here the field
    // is in neither story, and it no longer scrolls away mid-search.
    Column {
        id: heading
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }

        PageHeader {
            title: page.archived ? qsTr("Archived") : qsTr("Chats")
        }

        SearchField {
            id: searchField
            objectName: "chatSearchField"
            width: parent.width
            // The ordinary list searches chats, contacts and messages;
            // the archived list is a mode over chats alone.
            placeholderText: page.archived ? qsTr("Search chats") : qsTr("Search")
            onTextChanged: searchDebounce.restart()
        }
    }

    SilicaListView {
        id: listView
        objectName: "chatList"
        visible: !page.searching
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        // Delegates draw outside the list's own box otherwise, and the
        // banner below is translucent.
        clip: true
        model: chats.rows

        // Nothing in here applies to the archived list -- no profile
        // switch, no way further in, no new chat -- so the pulley itself
        // goes rather than opening onto an empty menu.
        PullDownMenu {
            objectName: "chatListPulley"
            visible: !page.archived
            enabled: !page.archived

            MenuItem {
                objectName: "profilesMenuItem"
                // Only worth offering where there is a choice to make.
                visible: !page.archived && page.accountCount > 1
                text: qsTr("Profiles")
                onClicked: pageStack.push(Qt.resolvedUrl("ProfilesPage.qml"),
                                          { currentAccountId: page.accountId })
            }
            MenuItem {
                objectName: "settingsMenuItem"
                visible: !page.archived
                text: qsTr("Settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                // The archived list is a mode, not a filter, so it is its
                // own page rather than a toggle on this one -- and it does
                // not offer a way further in.
                objectName: "archivedMenuItem"
                visible: !page.archived
                text: qsTr("Archived chats")
                onClicked: pageStack.push(Qt.resolvedUrl("ChatListPage.qml"), {
                    accountId: page.accountId,
                    archived: true
                })
            }
            MenuItem {
                objectName: "newChatMenuItem"
                visible: !page.archived
                text: qsTr("New chat")
                onClicked: pageStack.push(Qt.resolvedUrl("NewChatPage.qml"),
                                          { accountId: page.accountId })
            }
        }

        delegate: ListItem {
            id: delegateRoot
            contentHeight: body.height

            ChatListDelegate {
                id: body
                width: parent.width
                chatName: model.name
                preview: model.preview
                previewSender: model.preview_sender
                unreadCount: model.unread_count
                lastUpdated: model.last_updated
                isEncrypted: model.is_encrypted
                isPinned: model.is_pinned
                isMuted: model.is_muted
                isContactRequest: model.is_contact_request
                chatColor: model.color
                avatarPath: model.avatar_path
                summaryState: model.summary_state
            }

            menu: ContextMenu {
                MenuItem {
                    objectName: "markReadItem"
                    visible: !page.archived && model.unread_count > 0
                    text: qsTr("Mark as read")
                    onClicked: chats.mark_read(model.chat_id)
                }
                MenuItem {
                    objectName: "pinItem"
                    visible: !page.archived
                    text: model.is_pinned ? qsTr("Unpin") : qsTr("Pin")
                    onClicked: chats.set_pinned(model.chat_id, !model.is_pinned)
                }
                MenuItem {
                    objectName: "muteItem"
                    visible: !page.archived
                    text: model.is_muted ? qsTr("Unmute") : qsTr("Mute")
                    onClicked: chats.set_muted(model.chat_id, !model.is_muted)
                }
                // A request is not an ordinary chat: until it is
                // accepted the sender cannot be replied to, and the only
                // useful answers are yes and no.
                MenuItem {
                    objectName: "acceptItem"
                    visible: !page.archived && model.is_contact_request
                    text: qsTr("Accept")
                    onClicked: chats.accept_chat(model.chat_id)
                }
                MenuItem {
                    objectName: "blockItem"
                    visible: !page.archived && model.is_contact_request
                    text: qsTr("Block")
                    onClicked: chats.block_chat(model.chat_id)
                }
                MenuItem {
                    objectName: "archiveItem"
                    visible: !page.archived && !model.is_contact_request
                    text: qsTr("Archive")
                    onClicked: chats.archive(model.chat_id)
                }
                // The way back out. Without it an archived chat could only
                // be recovered by someone sending a message to it.
                MenuItem {
                    objectName: "unarchiveItem"
                    visible: page.archived
                    text: qsTr("Unarchive")
                    onClicked: chats.unarchive(model.chat_id)
                }
                MenuItem {
                    objectName: "deleteItem"
                    text: qsTr("Delete")
                    // The id is taken now, not read inside the callback: a
                    // message arriving moves this chat up the list, which
                    // is a remove and an insert, and the row this menu
                    // belongs to is destroyed. Silica runs the action on
                    // that destruction, and `model` no longer resolves.
                    onClicked: {
                        var doomed = model.chat_id
                        delegateRoot.remorseAction(qsTr("Deleting"),
                                                   function() {
                                                       chats.delete_chat(doomed)
                                                   })
                    }
                }
            }

            onClicked: {
                // Told before the push, so a message arriving during the
                // transition is not announced into the reader's face.
                notifier.viewingChatId = model.chat_id
                pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
                    accountId: page.accountId,
                    chatId: model.chat_id,
                    chatName: model.name
                })
            }
        }

        ViewPlaceholder {
            enabled: chats.count === 0
            text: qsTr("No chats yet")
            hintText: qsTr("Pull down to start one")
        }
    }

    SearchResultsList {
        id: resultsView
        objectName: "searchResults"
        visible: page.searching
        search: searchModel
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        onChatActivated: {
            notifier.viewingChatId = chatId
            pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chatId,
                chatName: chatName
            })
        }
        // No chat exists yet; the model answers on chat_ready. The name
        // is kept here because that answer carries only an id.
        onContactActivated: {
            page.pendingChatName = contactName
            searchModel.open_chat_with(contactId)
        }
    }

    Banner {
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
