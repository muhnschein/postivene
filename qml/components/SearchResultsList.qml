import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A search's answer: chats, then contacts, then messages, each under a
 * heading that says how many there were.
 *
 * One flat model with a `kind` role rather than three lists in a
 * flickable: QML sections a flat model, a view takes one model, and
 * nested flickables on Silica fight each other for the drag.
 *
 * Its own file rather than a second list inside the chat list page, so
 * each list's delegate can be checked against the one model it binds --
 * see tests/qml_syntax.rs.
 */
SilicaListView {
    id: root

    /// The SearchResults object whose rows these are.
    property QtObject search

    /// A chat result, or a message in one, was tapped.
    signal chatActivated(int chatId, string chatName)
    /// A contact result was tapped. There may be no chat with them yet,
    /// which is why this is not chatActivated -- and why the name comes
    /// with it: the chat that gets made has no title to read yet.
    signal contactActivated(int contactId, string contactName)

    // Delegates draw outside the list's own box otherwise.
    clip: true
    model: root.search ? root.search.rows : null

    section {
        property: "kind"
        delegate: SectionHeader {
            objectName: "searchSection"
            text: section === "chat"
                  ? qsTr("Chats (%1)").arg(root.search.chat_count)
                  : section === "contact"
                    ? qsTr("Contacts (%1)").arg(root.search.contact_count)
                    // Only the first of a big pile is listed, and saying
                    // how many were listed out of how many matched is the
                    // honest way to show that.
                    : root.search.message_total > root.search.message_count
                      ? qsTr("Messages (%1 of %2)").arg(root.search.message_count)
                                                   .arg(root.search.message_total)
                      : qsTr("Messages (%1)").arg(root.search.message_count)
        }
    }

    delegate: ListItem {
        contentHeight: resultBody.height

        SearchResultRow {
            id: resultBody
            width: parent.width
            title: model.title
            subtitle: model.subtitle
            ownColor: model.color
            picturePath: model.avatar_path
            timestamp: model.timestamp
        }

        onClicked: {
            if (model.kind === "contact") {
                root.contactActivated(model.contact_id, model.title)
            } else {
                // A message result opens the chat it is in. The view
                // lands on the newest message rather than on the hit --
                // jumping to one is its own piece of work.
                root.chatActivated(model.chat_id, model.title)
            }
        }
    }

    ViewPlaceholder {
        objectName: "noResultsPlaceholder"
        // `loaded` keeps this off the screen between a keystroke and its
        // answer, where the model is empty but nothing has been searched
        // for yet.
        enabled: root.search && root.search.loaded && root.search.count === 0
        text: qsTr("Nothing found")
    }
}
