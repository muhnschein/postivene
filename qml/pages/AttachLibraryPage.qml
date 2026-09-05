import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * The library: everything the phone has indexed, for attaching one of
 * them. The platform's own picker over pictures, videos, music and
 * documents at once, so the tray needs one entry for the lot rather than
 * one per kind.
 *
 * A page of its own, pushed by URL, rather than a Component inside
 * ConversationPage: `Sailfish.Pickers` types resolve when the file that
 * names them is loaded, so a type that is not there on some future release
 * would take the whole conversation down with it rather than one button.
 * The same shape as ChatPickerPage -- it reports what was chosen and lets
 * the caller decide what that means.
 */
ContentPickerPage {
    id: picker

    /// The absolute path of the chosen file.
    signal picked(string path)

    onSelectedContentPropertiesChanged: {
        // The core copies the file into its own blob directory, so the
        // picked one is free to go away afterwards.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
