import QtQuick 2.0
Item {
    default property alias contents: holder.data
    Item { id: holder }
    property bool busy: false
}
