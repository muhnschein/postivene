import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One contact, laid out like a chat-list row: the same round avatar in the
 * contact's own colour, the same spacing. The two lists sit one tap apart,
 * and looked like different applications.
 *
 * Name and address are whatever the other end supplied, so both are pinned
 * to PlainText -- see MessageDelegate.qml.
 */
Item {
    id: root

    property string displayName
    property string address
    property string ownColor
    property string picturePath
    /// An address contact cannot be written to encrypted.
    property bool isKeyContact: true
    property bool isVerified: false

    height: Math.max(avatar.height, addressLabel.y + addressLabel.height)
            + 2 * Theme.paddingMedium

    Avatar {
        id: avatar
        objectName: "contactAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.displayName
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    Label {
        id: nameLabel
        objectName: "contactName"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: root.width - x - Theme.horizontalPageMargin
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        // The same marks the chat list uses: a mail icon for a contact
        // that cannot be encrypted to, a tick for one checked in person.
        text: (root.isKeyContact ? "" : "✉ ") + root.displayName
              + (root.isVerified ? " ✓" : "")
    }

    Label {
        id: addressLabel
        objectName: "contactAddress"
        x: nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: nameLabel.width
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.address
    }
}
