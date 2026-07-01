import QtQuick 2.0
import Sailfish.Silica 1.0

CoverBackground {
    Label {
        anchors.centerIn: parent
        text: "Postivene"
        font.pixelSize: Theme.fontSizeLarge
    }

    Label {
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        text: core.status
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        truncationMode: TruncationMode.Fade
    }
}
