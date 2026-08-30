pragma Singleton
import QtQuick 2.0

// The constants the pages read. Values are arbitrary; only that a binding
// resolves is being checked.
QtObject {
    property int paddingSmall: 4
    property int paddingMedium: 8
    property int paddingLarge: 16
    property int horizontalPageMargin: 24
    property int itemSizeExtraSmall: 40
    property int itemSizeSmall: 60
    property int itemSizeMedium: 90
    property int itemSizeLarge: 120
    property int itemSizeExtraLarge: 180
    property int fontSizeExtraSmall: 10
    property int fontSizeSmall: 12
    property int fontSizeMedium: 18
    property int fontSizeLarge: 24
    property int fontSizeHuge: 48
    property color primaryColor: "#ffffff"
    property color secondaryColor: "#a0a0a0"
    property color highlightColor: "#80c0ff"
    property color highlightBackgroundColor: "#2060a0"
    property color errorColor: "#ff4040"

    function rgba(color, opacity) {
        return Qt.rgba(color.r, color.g, color.b, opacity)
    }
}
