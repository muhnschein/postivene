import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica's file browser, which the app uses filtered to one extension.
// What the app reads is the chosen file, and what it sets is the filter
// and the heading; a test drives it by assigning to the first.
Page {
    property var nameFilters: []
    property string title: ""
    property var selectedContentProperties: ({ filePath: "" })
}
