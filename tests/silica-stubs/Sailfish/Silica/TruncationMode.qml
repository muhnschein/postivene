import QtQuick 2.0

// Silica's enum. Declared with QML's own `enum`, which the host Qt (5.15)
// supports; the device's Qt 5.6 never sees the stubs.
//
// `None` is not Silica's -- it has no such value, a Label simply defaults
// to not truncating. It is here so the stub Label has something to mean
// "left alone".
QtObject {
    enum Mode { None, Fade, Elide }
}
