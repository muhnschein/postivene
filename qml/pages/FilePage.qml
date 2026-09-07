import QtQuick 2.0
import Sailfish.Silica 1.0
import Postivene 1.0
import "../components"
import "../js/Format.js" as Format

/*
 * A file somebody sent, and what can be done with it.
 *
 * A tap on a picture or a video opens it here; a tap on anything else
 * used to hand it straight to the phone, which for a `.md`, a `.log` or
 * a patch means nothing happens at all -- no app claims those, and the
 * handover fails silently. So the tap comes here instead, where the file
 * says what it is and offers the two things there are to do with it:
 * open it with whatever else is installed, or keep a copy somewhere the
 * reader can find it again.
 *
 * And when it is text, it is simply shown. That is what the reader
 * wanted from the file in the first place, and this app can do it
 * without asking anything of the phone.
 */
Page {
    id: page

    /// The file, as a `file://` URL.
    property url fileUrl
    /// Its name, as the sender gave it.
    property string fileName: ""
    /// Its type, as the core read it. May be empty.
    property string fileMime: ""
    /// Its size in bytes, 0 when unknown.
    property real fileBytes: 0
    /// Whether this is a file the page can show as words. Decided by the
    /// shim, once, from the type and the name (`full_text.rs`), so the
    /// row, its menu and this page cannot disagree about it.
    property bool isText: false
    /// 0 draws Markdown, 1 takes its markers out, 2 shows it as written.
    property int markdownMode: 2

    allowedOrientations: Orientation.All

    // Only for a file this page will show: reading a video into a string
    // to display none of it is a megabyte of nothing.
    FullText {
        id: whole
        objectName: "fullText"
        file_path: page.isText ? page.fileUrl : ""
        onError: {
            notice.tone = "error"
            notice.show(message)
        }
    }

    // A copy where the reader can find it. Downloads rather than
    // Pictures: this page is what is left once the picture and the video
    // have pages of their own, so what arrives here is a document.
    FileSaver {
        id: saver
        objectName: "saver"
        onSaved: {
            notice.tone = "info"
            notice.show(qsTr("Saved to Downloads"))
        }
        onError: {
            notice.tone = "error"
            notice.show(message)
        }
    }

    /// Whether the body is drawn from the shim's rendering; see
    /// MessageDelegate.
    readonly property bool drawsStyled: page.markdownMode === 0
                                        && whole.styled_text.length > 0
    readonly property string shownText: page.drawsStyled
                                        ? whole.styled_text
                                        : page.markdownMode === 1 && whole.plain_text.length > 0
                                          ? whole.plain_text
                                          : whole.text

    SilicaFlickable {
        id: flickable
        objectName: "fileFlickable"
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "openExternally"
                //: Hands the attachment to whatever else on the phone
                //: handles files of its kind.
                text: qsTr("Open in another app")
                onClicked: Qt.openUrlExternally(page.fileUrl)
            }
            MenuItem {
                objectName: "saveToDevice"
                text: qsTr("Save to Downloads")
                onClicked: saver.save(page.fileUrl, StandardPaths.download)
            }
        }

        VerticalScrollDecorator {}

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "fileHeader"
                //: Heading of the page showing one received file.
                title: qsTr("File")
            }

            // The name the sender gave it, and what it is. Labels of
            // their own rather than the header's title and description,
            // which draw what they are given as markup -- and both of
            // these are the sender's words: see ConversationHeader.
            Label {
                objectName: "fileNameLabel"
                visible: page.fileName.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: page.fileName
            }

            Label {
                objectName: "fileFactsLabel"
                visible: text.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                // The size is this app's own arithmetic; the type is
                // whatever came with the message.
                text: page.fileBytes > 0
                      ? (page.fileMime.length > 0
                         ? Format.readableSize(page.fileBytes) + " · " + page.fileMime
                         : Format.readableSize(page.fileBytes))
                      : page.fileMime
            }

            Label {
                objectName: "clippedLabel"
                visible: whole.clipped
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                //: Shown above a text file too long to show whole.
                text: qsTr("Only the beginning is shown. Open the file "
                           + "elsewhere to read all of it.")
            }

            Label {
                id: bodyLabel
                objectName: "bodyLabel"
                visible: page.shownText.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.primaryColor
                linkColor: Theme.highlightColor
                // Plain, unless the shim rendered it: see MessageDelegate.
                textFormat: page.drawsStyled ? Text.StyledText : Text.PlainText
                text: page.shownText
                onLinkActivated: Qt.openUrlExternally(link)
            }

            // Nothing to show, which is most files: say so, since the
            // two things to do with it are behind the pull-down menu and
            // an empty page gives no reason to look there.
            Label {
                objectName: "nothingToShowLabel"
                visible: !page.isText
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                //: Shown for an attachment this app cannot display, above
                //: the two things that can be done with it.
                text: qsTr("Postivene cannot show this kind of file. Pull "
                           + "down to open it in another app or save a "
                           + "copy.")
            }
        }
    }

    // Where the copy went, or why there is none.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
