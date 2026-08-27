import QtQuick 2.0
Item {
    property QtObject icon: QtObject { property string source }
    signal clicked()
    implicitWidth: 60
    implicitHeight: 60
}
