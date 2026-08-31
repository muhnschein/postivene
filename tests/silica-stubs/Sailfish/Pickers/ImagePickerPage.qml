import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica's gallery picker. What the app reads is the one property, so
// that is all this carries; a test drives it by assigning to it.
Page {
    property var selectedContentProperties: ({ filePath: "" })
}
