import QtQuick 2.0

// Silica's enum. Declared with QML's own `enum`, which the host Qt (5.15)
// supports; the device's Qt 5.6 never sees the stubs.
QtObject {
    enum Status { Inactive, Activating, Active, Deactivating }
}
