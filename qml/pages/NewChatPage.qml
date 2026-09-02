import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Pick someone to talk to: known contacts in the list, and the ways to get
 * a new one in the pulley menu, as on the chat list.
 *
 * "New contact" opens the scanner rather than an address form. An address
 * alone produces a chat that cannot be encrypted, and an invite is how a
 * Delta Chat contact is actually added: the other person's code, held up
 * to the camera, or their link typed into the panel the scanner offers.
 * The scanner stays up while the invite is followed, and the chat
 * replaces it and this page together.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    // Not bound straight to the field: a round trip per keystroke asks the
    // core four times to type "anna", and only the last answer is wanted.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: contacts.query = searchField.text
    }

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: {
            page.errorMessage = message
            // Back from the scanner, if it is what asked, to where the
            // message is.
            if (pageStack.currentPage !== page) {
                pageStack.pop(page)
            }
        }
        // Every route ends the same way: open the chat that now exists,
        // above the page that opened this one. The scanner may be on top
        // of this page, and goes with it.
        onChat_ready: pageStack.replaceAbove(pageStack.previousPage(page),
                                             Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: qsTr("Chat")
        })
    }

    // The scanner, pushed by URL and connected to, the way the pickers
    // are: ScanPage is the only file that names a Camera, so a device
    // without one costs this entry rather than the page.
    function scan() {
        var scanner = pageStack.push(Qt.resolvedUrl("ScanPage.qml"))
        if (scanner) {
            scanner.scanned.connect(function(text) {
                page.errorMessage = ""
                contacts.join_by_invite(text)
            })
        }
    }

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    // Outside the list for the reason ChatListPage documents: a field in
    // a view's `header` lives inside the flickable and its id does not
    // resolve from the page, so every reference to it below was reading
    // an undefined name.
    // One flickable for the whole page, owning the pulley.
    //
    // A PullDownMenu draws at the top of the flickable that owns
    // it, and the list starts below the search field -- so a pulley
    // on the list opened below the field, or inside it. It does not
    // scroll: the field has to stay out of a view whose contents
    // change on every keystroke, which is what took the keyboard
    // away mid-search.
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        PullDownMenu {
            MenuItem {
                objectName: "newGroupButton"
                text: qsTr("New group")
                onClicked: pageStack.push(Qt.resolvedUrl("NewGroupPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                // An address alone produces a chat that cannot be
                // encrypted, so adding a contact starts from their code
                // -- which is how a Delta Chat contact is actually added.
                objectName: "newContactButton"
                text: qsTr("New contact")
                onClicked: page.scan()
            }
        }

        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                title: qsTr("New chat")
            }

            SearchField {
                id: searchField
                objectName: "searchField"
                width: parent.width
                placeholderText: qsTr("Search contacts")
                onTextChanged: searchDebounce.restart()
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }

        SilicaListView {
            id: listView
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            model: contacts.rows


            // No context menu: picking a contact is the only thing to do
            // with one here.
            delegate: ListItem {
                objectName: "contactRow"
                contentHeight: body.height

                ContactRow {
                    id: body
                    width: parent.width
                    displayName: model.display_name
                    ownColor: model.color
                    picturePath: model.avatar_path
                    isKeyContact: model.is_key_contact
                    isVerified: model.is_verified
                }

                onClicked: contacts.open_chat_with(model.contact_id)
            }

            ViewPlaceholder {
                enabled: contacts.count === 0
                text: qsTr("No contacts yet")
                hintText: qsTr("Pull down to scan someone's invite")
            }
        }
    }
}
