pragma Singleton
import QtQuick 2.0

// Silica's screen singleton. `topCutout` is the notch or hole in the
// display, as a rect in portrait pixels, and all zeros on a screen
// without one; writable here so a test can put one there.
QtObject {
    property int width: 1080
    property int height: 2520
    property rect topCutout: Qt.rect(0, 0, 0, 0)
}
