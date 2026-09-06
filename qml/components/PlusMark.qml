import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The mark on a row that adds one more of something -- a member, a
 * profile -- standing where the picture of the thing itself stands on
 * the rows above it.
 *
 * The theme's own plus, and nothing behind it. It used to sit on a disc
 * of the highlight colour, avatar-shaped, so the row would line up: the
 * theme draws that icon with a ring of its own, and the two together
 * read as a circle inside a circle. The space is still an avatar's, so
 * the text beside it still lines up with the names above.
 */
Item {
    id: mark

    width: Theme.itemSizeSmall
    height: width

    Image {
        objectName: "plusIcon"
        anchors.centerIn: parent
        source: "image://theme/icon-m-add"
    }
}
