import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * A webxdc app off the phone: the platform's file browser, showing only
 * the .xdc files it finds.
 *
 * The library picker beside it cannot serve here -- it lists what the
 * media index knows about, and an app somebody sent is a file the index
 * has no opinion on -- so this is the browser, filtered.
 *
 * A page of its own, pushed by URL, for the reason every Attach*Page is
 * one: `Sailfish.Pickers` types resolve when the file naming them is
 * loaded, so a type that is not there costs this button rather than the
 * conversation.
 */
FilePickerPage {
    id: picker

    /// The absolute path of the chosen app.
    signal picked(string path)

    // The heading is Silica's own: the page is its file browser, and it
    // names itself in every language the platform has.
    nameFilters: ["*.xdc"]

    onSelectedContentPropertiesChanged: {
        // The core copies the file into its own blob directory, so the
        // picked one is free to go away afterwards.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
