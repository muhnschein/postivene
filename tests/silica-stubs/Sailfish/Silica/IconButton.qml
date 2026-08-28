import QtQuick 2.0

// `icon` has to be an alias to a real item: QML only accepts
// `icon.source: ...` for a value type or an alias, not for a plain
// object-typed property.
Item {
    property alias icon: iconImage
    Image { id: iconImage; visible: false }
    signal clicked()
    implicitWidth: 60
    implicitHeight: 60
}
