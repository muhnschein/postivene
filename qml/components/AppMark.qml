import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * What an app is drawn as: nine squares in a grid, which is the mark
 * every phone uses for "apps".
 *
 * Drawn rather than named. Silica's theme has an icon for most things and
 * this app asks it only for names it has been seen to have -- an icon
 * that is not there is a button nobody can see -- and a webxdc is new
 * enough that no such name can be relied on. Nine rectangles cost
 * nothing, scale with whatever size they are given, and take the colour
 * of whatever they are put in.
 *
 * Laid out by bindings rather than a Grid: a positioner sizes itself in a
 * polish pass, which never runs headlessly, so a mark built from one
 * measures as nothing in a test.
 */
Item {
    id: root

    /// How big the whole mark is. The squares divide it: three of them
    /// and two gaps make exactly this.
    property real size: Theme.iconSizeSmall
    /// What colour to draw it in.
    property color color: Theme.primaryColor

    readonly property real cell: root.size / 4
    readonly property real gap: root.size / 8

    width: root.size
    height: root.size

    Repeater {
        model: 9

        Rectangle {
            width: root.cell
            height: root.cell
            radius: root.cell / 4
            x: (index % 3) * (root.cell + root.gap)
            y: Math.floor(index / 3) * (root.cell + root.gap)
            color: root.color
        }
    }
}
