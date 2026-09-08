import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * What to do with a file a webxdc app has just handed over.
 *
 * The webxdc call is `sendToChat`, and its name is the whole of what the
 * API offers an app that wants to put a file somewhere -- there is no
 * "save this" in the spec at all. But the button an app draws for it is a
 * download, and what a reader means by it is the file, on their phone:
 * `sharer`'s is literally a download arrow. So the question here is the
 * one they were actually asking, and the chat is not one of the answers.
 *
 * Two ways out and no default. Opening hands it to whatever the phone
 * uses for its kind and keeps it in the cache; saving puts a copy in
 * Downloads, where the file manager looks. Backing out leaves the file in
 * the cache, which is emptied with the app.
 */
Dialog {
    id: dialog

    /// The file the app wrote, as a path.
    property string filePath: ""
    /// What the app called it. The app's own words, so never a header.
    property string fileName: ""

    /// Which way out was taken. The page does the work: this only asks.
    signal openChosen()
    signal saveChosen()

    // Nothing to accept: both ways out are rows, and the header's own
    // cancel is the third. An accept with no meaning would be a fourth.
    canAccept: false

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            //: Shown over the two things that can be done with a file a
            //: webxdc app has just produced.
            title: qsTr("File from the app")
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "handoverName"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            // The name is the app's, and an app is not to be trusted with
            // markup: see MessageDelegate.
            textFormat: Text.PlainText
            text: dialog.fileName
        }

        BackgroundItem {
            objectName: "handoverOpen"
            onClicked: {
                dialog.openChosen()
                pageStack.pop()
            }

            Label {
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                color: parent.down ? Theme.highlightColor : Theme.primaryColor
                //: Hand the file to whatever the phone opens its kind with.
                text: qsTr("Open")
            }
        }

        BackgroundItem {
            objectName: "handoverSave"
            onClicked: {
                dialog.saveChosen()
                pageStack.pop()
            }

            Label {
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                color: parent.down ? Theme.highlightColor : Theme.primaryColor
                //: Keep a copy of the file where the file manager looks.
                text: qsTr("Save")
            }
        }
    }
}
