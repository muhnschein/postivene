pragma Singleton
import QtQuick 2.0

// The display, as Silica reports it: the stub Page's own size, and the
// cutout the real one reports on a phone that has a notch. Empty here
// unless a test says otherwise -- what is being checked is that the app
// reads it, not what any phone answers.
QtObject {
    property int width: 540
    property int height: 960
    property rect topCutout: Qt.rect(0, 0, 0, 0)
}
