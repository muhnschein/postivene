import QtQuick 2.0

// The camera's focus settings, a type of its own so the page's grouped
// `focus { ... }` assignment resolves against declared properties. The
// modes the page names are the real type's enums and read as undefined
// here, which the `var` properties take without complaint.
QtObject {
    property var focusMode
    property var focusPointMode
    property point customFocusPoint
}
