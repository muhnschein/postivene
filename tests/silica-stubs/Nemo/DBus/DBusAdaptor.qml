import QtQuick 2.0

// Stands in for Nemo.DBus's DBusAdaptor, which puts an object on the
// session bus on a device. Nothing is registered here: it holds the name
// it would own and the functions a caller would reach, so a test can call
// the one a notification's tap names.
QtObject {
    property string service
    property string path
    property string iface
    property string xml
}
