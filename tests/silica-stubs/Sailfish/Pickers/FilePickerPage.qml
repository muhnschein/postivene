import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica's file-system picker. Same one property the app reads as
// ImagePickerPage; a test drives it by assigning to it.
Page {
    property var selectedContentProperties: ({ filePath: "" })
}
