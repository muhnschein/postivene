import QtQuick 2.0

// The platform thumbnailer, which produces a poster frame for a video
// without this app decoding one. Nothing here can generate a thumbnail, so
// this carries the properties the preview sets and draws nothing.
Item {
    property url source
    property string mimeType
    property int fillMode: 0
    property size sourceSize
}
