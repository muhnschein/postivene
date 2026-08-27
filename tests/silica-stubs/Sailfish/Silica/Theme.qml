pragma Singleton
import QtQuick 2.0

// Silica's Theme, reduced to the constants the pages read. The values are
// arbitrary: nothing here is asserting layout, only that a binding resolves.
QtObject {
    property int paddingSmall: 4
    property int paddingMedium: 8
    property int paddingLarge: 16
    property int horizontalPageMargin: 24
    property int itemSizeSmall: 60
    property int itemSizeMedium: 90
    property int fontSizeExtraSmall: 10
    property int fontSizeSmall: 12
    property int fontSizeLarge: 24
    property color primaryColor: "#ffffff"
    property color secondaryColor: "#a0a0a0"
    property color highlightColor: "#80c0ff"
    property color errorColor: "#ff4040"
}
