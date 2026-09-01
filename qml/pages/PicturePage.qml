// 2.5 rather than 2.0 for Image.autoTransform; see AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0

/*
 * One picture, as big as the screen will show it.
 *
 * Tapping an image used to hand it to whatever the system thought handled
 * the type, which left Postivene and, on a device, failed there. Showing it
 * here is a page and a Flickable; the way out to another app stays in the
 * pull-down for the cases this cannot do anything with.
 *
 * The picture is fitted at zoom 1 and multiplied from there, and the
 * flickable's content is the larger of the picture and the view -- so
 * panning does nothing until there is something to pan, and the picture
 * stays centred while there is not.
 */
Page {
    id: page

    /// The file, already encoded: see AttachmentPreview.fileUrl for why the
    /// path is not concatenated into one here.
    property url fileUrl
    property string fileName
    /// Image, Gif or Sticker -- the core's own enum, as everywhere else.
    property string viewType: "Image"

    readonly property bool isAnimated: page.viewType === "Gif"

    /// How much bigger than fitted the picture is drawn. 1 fits it.
    property real zoom: 1
    readonly property real maximumZoom: 5

    /// The width the picture is drawn at when it is fitted to the page.
    ///
    /// Falls back to the page's own width until the picture has been
    /// decoded, which is also what keeps this from dividing by zero.
    readonly property real fittedWidth: {
        if (picture.implicitWidth <= 0 || picture.implicitHeight <= 0) {
            return page.width
        }
        return Math.min(page.width,
                        page.height * picture.implicitWidth / picture.implicitHeight)
    }
    readonly property real fittedHeight:
        picture.implicitWidth > 0
        ? page.fittedWidth * picture.implicitHeight / picture.implicitWidth
        : page.height

    /// Kept inside the content. The flickable would do this itself, but
    /// only once it has been laid out, and this runs before that.
    function within(value, limit) {
        return Math.max(0, Math.min(Math.max(0, limit), value))
    }

    /// Where the picture's left edge sits when it is narrower than the view.
    function inset(size, viewport) {
        return Math.max(0, (Math.max(size, viewport) - size) / 2)
    }

    /// Change the zoom, keeping whatever is under (`viewX`, `viewY`) under
    /// it afterwards.
    ///
    /// The coordinates are in the view, not in the picture. Without this
    /// the picture simply grows about its own top-left corner, and zooming
    /// in on a face in the bottom right takes the reader to the top left
    /// and leaves them to find their way back.
    function zoomAt(target, viewX, viewY) {
        var next = Math.max(1, Math.min(page.maximumZoom, target))
        var wide = page.fittedWidth * page.zoom
        var high = page.fittedHeight * page.zoom
        if (wide <= 0 || high <= 0 || next === page.zoom) {
            return
        }
        // Where that point sits in the picture, 0..1, before the change.
        var across = (flick.contentX + viewX - page.inset(wide, flick.width)) / wide
        var down = (flick.contentY + viewY - page.inset(high, flick.height)) / high

        page.zoom = next

        var wideAfter = page.fittedWidth * next
        var highAfter = page.fittedHeight * next
        // And where it has to be for that point to be under the same place
        // on the screen.
        flick.contentX = page.within(
            page.inset(wideAfter, flick.width) + across * wideAfter - viewX,
            Math.max(wideAfter, flick.width) - flick.width)
        flick.contentY = page.within(
            page.inset(highAfter, flick.height) + down * highAfter - viewY,
            Math.max(highAfter, flick.height) - flick.height)
    }

    /// What a double tap does: all the way in on what was tapped, or all
    /// the way back out.
    function toggleZoom(viewX, viewY) {
        page.zoomAt(page.zoom > 1 ? 1 : 3, viewX, viewY)
    }

    // Behind the picture rather than the theme's own background: a photo
    // with anything light in it reads better against black, and the bars
    // beside a picture that does not fill the screen are not the page.
    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    SilicaFlickable {
        id: flick
        objectName: "pictureFlick"
        anchors.fill: parent
        contentWidth: Math.max(flick.width, frame.width)
        contentHeight: Math.max(flick.height, frame.height)
        clip: true

        PullDownMenu {
            MenuItem {
                objectName: "openExternally"
                //: Hands the attachment to whatever else on the phone
                //: handles files of its kind.
                text: qsTr("Open in another app")
                onClicked: Qt.openUrlExternally(page.fileUrl)
            }
        }

        PinchArea {
            id: pincher
            width: flick.contentWidth
            height: flick.contentHeight
            pinch.minimumScale: 1
            pinch.maximumScale: page.maximumZoom

            /// Where the zoom was when the fingers went down: a pinch
            /// reports its scale relative to its own start, not to the
            /// picture.
            property real startZoom: 1
            /// And where on the screen they went down, which is the point
            /// the picture has to stay still under.
            property real focusX: 0
            property real focusY: 0

            onPinchStarted: {
                pincher.startZoom = page.zoom
                // This area is the flickable's content, so its coordinates
                // are content coordinates; the view's are what zoomAt
                // wants.
                pincher.focusX = pinch.startCenter.x - flick.contentX
                pincher.focusY = pinch.startCenter.y - flick.contentY
            }
            onPinchUpdated: page.zoomAt(pincher.startZoom * pinch.scale,
                                        pincher.focusX, pincher.focusY)

            Item {
                id: frame
                width: page.fittedWidth * page.zoom
                height: page.fittedHeight * page.zoom
                // Centred while the picture is smaller than the view, and
                // at the origin once it is bigger and the flickable has
                // taken over.
                x: Math.max(0, (flick.contentWidth - frame.width) / 2)
                y: Math.max(0, (flick.contentHeight - frame.height) / 2)

                // The same layering as the message row: an Image reads the
                // first frame of a GIF whatever happens to its movie, so it
                // sizes the page and shows the still, and the animation
                // covers it when there is one to play. See
                // AttachmentPreview.
                Image {
                    id: picture
                    objectName: "fullPicture"
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    source: page.fileUrl
                }

                AnimatedImage {
                    objectName: "fullAnimation"
                    anchors.fill: parent
                    visible: page.isAnimated
                    fillMode: Image.PreserveAspectFit
                    playing: visible
                    source: visible ? page.fileUrl : ""
                }

                MouseArea {
                    id: tap
                    anchors.fill: parent
                    onDoubleClicked: {
                        var point = tap.mapToItem(flick, mouse.x, mouse.y)
                        page.toggleZoom(point.x, point.y)
                    }
                }
            }
        }

        // Only while the picture is still coming: a decode that fails
        // leaves the page black, and a spinner that never stops would say
        // it was still trying.
        BusyIndicator {
            objectName: "pictureBusy"
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: picture.status === Image.Loading
        }
    }
}
