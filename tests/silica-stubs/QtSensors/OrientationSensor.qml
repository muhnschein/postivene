import QtQuick 2.0

// The device's orientation sensor. Nothing turns here: a test hands it a
// reading through `report`, with OrientationReading's own values (TopUp
// 1, TopDown 2, LeftUp 3, RightUp 4), the way the hardware would. A new
// object is a new reading, so assigning it raises readingChanged.
QtObject {
    property bool active: false
    property var reading: null
    function report(orientation) { reading = { orientation: orientation } }
}
