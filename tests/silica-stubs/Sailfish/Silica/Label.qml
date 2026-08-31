import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica fades or elides a Label that does not fit the width it was
// given. Qt's own Text does neither by default: it keeps the width and
// paints straight over the edge. Modelling that here is what lets a test
// see a label whose width binding never resolved -- which is what a
// binding loop leaves behind, and what put a search result off the side
// of the screen.
Text {
    property int truncationMode: TruncationMode.None
    elide: truncationMode === TruncationMode.None ? Text.ElideNone : Text.ElideRight
}
