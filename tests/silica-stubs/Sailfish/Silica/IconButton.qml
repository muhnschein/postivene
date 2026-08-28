import QtQuick 2.0

// `icon` has to be an alias to a real item: QML only accepts
// `icon.source: ...` for a value type or an alias, not for a plain
// object-typed property.
Item {
    property alias icon: iconImage
    Image { id: iconImage; visible: false }
    signal clicked()
    // Real, not implicit: `implicitHeight` on a plain Item does not set
    // `height`, so a stub carrying only that measures as nothing and hides
    // every layout bug an icon button's size could cause.
    implicitWidth: 60
    implicitHeight: 60
    width: 60
    height: 60
}
