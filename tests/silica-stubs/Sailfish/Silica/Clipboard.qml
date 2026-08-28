pragma Singleton
import QtQuick 2.0
// Silica's Clipboard is a C++ singleton; only `text` is used here.
QtObject {
    property string text: ""
}
