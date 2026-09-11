import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"
import "components"

ApplicationWindow {
    id: appWindow

    initialPage: Component { WelcomePage {} }
    // Nothing is handled here any more: the cover's action was removed
    // along with the status label it was drawn on top of, and tapping
    // the cover already opens the app.
    cover: Component { CoverPage {} }

    /// The profile the app is on, as the chat list last opened it.
    ///
    /// Kept here because a share arrives at the window rather than at a
    /// page: what is shared goes into a chat of whichever profile is
    /// being read, and 0 means there is none yet -- the app is still on
    /// its way in, and there is nothing to share into.
    property int accountId: 0

    /// Something was shared to the app. Ask which chat it is for, then
    /// open that chat with it already in hand: the file on the
    /// attachment bar, the text in the field, for the reader to add to
    /// and send.
    function shareInto(filePath, text) {
        if (appWindow.accountId === 0) {
            return
        }
        var picker = pageStack.push(
            Qt.resolvedUrl("pages/ChatPickerPage.qml"),
            { accountId: appWindow.accountId, closeOnPick: false })
        if (!picker) {
            return
        }
        picker.chatPicked.connect(function (chatId, chatName) {
            appWindow.openShared(chatId, chatName, filePath, text)
        })
    }

    /// Open the chat a share was pointed at, carrying what was shared.
    ///
    /// Replacing the picker rather than pushing over it: what the reader
    /// asked for is the chat, and the way back from it is to where they
    /// were before the share.
    function openShared(chatId, chatName, filePath, text) {
        pageStack.replace(Qt.resolvedUrl("pages/ConversationPage.qml"), {
            accountId: appWindow.accountId,
            chatId: chatId,
            chatName: chatName,
            sharedFile: filePath,
            sharedText: text
        })
    }

    // The share sheet's side of the app. Loaded rather than declared:
    // `Sailfish.Share` resolves only on a release that ships it, and an
    // import that does not resolve would otherwise take down the window
    // every page is loaded into. See qml/share/ShareTarget.qml.
    Loader {
        id: shareTarget
        objectName: "shareTarget"
        source: Qt.resolvedUrl("share/ShareTarget.qml")
    }

    // The two settings the core has to be told about: each applies to
    // every profile, and follows its key as the settings page changes it.
    Binding {
        target: core
        property: "download_limit"
        value: Settings.downloadLimit
    }
    Binding {
        target: core
        property: "delete_device_after"
        value: Settings.deleteDeviceAfter
    }

    Component.onCompleted: {
        core.start(rpcServerPath)
    }
}
