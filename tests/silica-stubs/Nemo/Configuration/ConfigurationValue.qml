import QtQuick 2.0

// Stands in for Nemo.Configuration's ConfigurationValue, which reads and
// writes one dconf key on a device. Nothing is stored here: the value
// starts at its default and holds whatever a page writes to it, which is
// what a test reads back.
QtObject {
    property string key
    property var defaultValue
    property var value: defaultValue
}
