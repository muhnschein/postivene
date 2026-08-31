import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica's own SectionHeader insets itself from the page margin on both
// sides and right-aligns its text in what is left, with its x at the
// left margin.
//
// Modelled rather than simplified, because that shape is exactly what a
// page can get wrong: assign `width: parent.width` and the x stays put
// while the right edge -- and the right-aligned text on it -- moves a
// whole margin past the screen. A device screenshot showed the last two
// characters of both headings cut off at the edge, which is that margin.
// A bare Item with no x and no alignment cannot show any of it.
Item {
    id: header

    property alias text: label.text

    x: Theme.horizontalPageMargin
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : 0
    implicitHeight: 60
    height: implicitHeight

    Label {
        id: label
        objectName: "sectionHeaderText"
        anchors.right: parent.right
        horizontalAlignment: Text.AlignRight
    }
}
