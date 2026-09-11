// 2.3 for `Image.mipmap`; Harbour allows up to 2.6 (ci/harbour/).
import QtQuick 2.3
import Sailfish.Silica 1.0

/*
 * A field of faces in the ambience's colours: what the first screen is
 * drawn over, after the cover's grid of everyone. Nobody is known yet,
 * so the faces are made up, and they are an IMAGE -- painted ahead of
 * time by tools/faces/ and shipped in qml/art/ -- rather than the
 * cover's avatars laid out again. Forty of those are forty masked,
 * desaturated, tinted textures, which is the wrong first impression;
 * a picture costs one texture and one pass, cannot half-arrive, and
 * looks the same on every phone.
 *
 * # One mask, every ambience
 *
 * What ships has no colour in it. Its red channel is how much ink each
 * pixel of a grey face carries, its green the same for a face that is
 * lit, and this draws red in the theme's primary colour and green in
 * its highlight -- the two colours the cover draws its own grid in, so
 * the page reads as the cover does on whatever ambience the phone
 * wears, a light one included. The grey faces are dimmed to `ink`, as
 * the cover dims its own; the lit ones are not.
 *
 * The same shader clears a box in the middle for the words, which is
 * geometry only the page knows: nothing within `clearRadius` of the
 * box, the faces back at full strength `clearFeather` beyond it. Cut
 * here rather than baked in, so the room is exactly where the words
 * are rather than where they were guessed to be.
 *
 * The mask keeps its own proportions and is centred, never stretched:
 * the shader crops it to whatever it is asked to fill, so one master
 * per orientation serves every screen from 16:9 to 21:9.
 */
Item {
    id: field

    /// The mask to draw, as a URL relative to the file that sets it.
    property url source

    /// The colours: grey faces in the first, lit ones in the second.
    property color colour: Theme.primaryColor
    property color litColour: Theme.highlightColor
    /// How much of each colour: the cover draws its grey faces at six
    /// tenths and whoever has written at full strength.
    property real ink: 0.6
    property real litInk: 1.0

    /// The box the faces clear, centred on `clearX`, `clearY`, with
    /// rounded corners `clearRadius` out from its edges and a fade of
    /// `clearFeather` beyond that. Off while the box has no size and
    /// no radius.
    property real clearX: 0
    property real clearY: 0
    property real clearWidth: 0
    property real clearHeight: 0
    property real clearRadius: 0
    property real clearFeather: 0

    /// True once there is something to draw.
    readonly property bool ready: mask.status === Image.Ready

    // The mask, loaded off the main thread and never drawn itself --
    // only sampled. Mipmapped, because every phone narrower than the
    // master scales it down, and a face scaled down without them
    // shimmers at its edges.
    Image {
        id: mask
        objectName: "faceMask"
        source: field.source
        visible: false
        asynchronous: true
        mipmap: true
    }

    ShaderEffect {
        objectName: "faceShader"
        anchors.fill: parent
        // Nothing to sample until it is there; a shader over an empty
        // texture draws a block of colour.
        visible: field.ready

        property variant source: mask
        property color tint: field.colour
        property color litTint: field.litColour
        property real ink: field.ink
        property real litInk: field.litInk
        property variant size: Qt.size(Math.max(1, field.width), Math.max(1, field.height))
        property real srcAspect: mask.implicitHeight > 0
                                 ? mask.implicitWidth / mask.implicitHeight
                                 : 1
        property variant clearAt: Qt.point(field.clearX, field.clearY)
        property variant clearHalf: Qt.size(field.clearWidth / 2, field.clearHeight / 2)
        property real clearRadius: field.clearRadius
        property real clearFeather: field.clearFeather

        // Fixed text: nothing foreign is anywhere near it. Qt hands the
        // colours over premultiplied, and the theme's are opaque, so
        // their rgb is the colour itself; what goes out is
        // premultiplied too, which is what the scene graph composites.
        fragmentShader: "
            varying highp vec2 qt_TexCoord0;
            uniform sampler2D source;
            uniform lowp vec4 tint;
            uniform lowp vec4 litTint;
            uniform lowp float ink;
            uniform lowp float litInk;
            uniform highp vec2 size;
            uniform highp float srcAspect;
            uniform highp vec2 clearAt;
            uniform highp vec2 clearHalf;
            uniform highp float clearRadius;
            uniform highp float clearFeather;
            uniform lowp float qt_Opacity;

            void main() {
                // Cover the item with the mask, keeping the mask's own
                // proportions and centring it, so the field is cropped
                // rather than stretched.
                highp float itemAspect = size.x / size.y;
                highp vec2 span = vec2(1.0, 1.0);
                if (itemAspect > srcAspect) {
                    span.y = srcAspect / itemAspect;
                } else {
                    span.x = itemAspect / srcAspect;
                }
                lowp vec4 face = texture2D(source, (qt_TexCoord0 - 0.5) * span + 0.5);

                // The room for the words: the distance out from the
                // box's edge, which is a rounded box once the radius
                // is taken off it.
                highp float keep = 1.0;
                if (clearRadius > 0.0 || clearHalf.x > 0.0) {
                    highp vec2 px = qt_TexCoord0 * size;
                    highp vec2 beyond = max(abs(px - clearAt) - clearHalf, vec2(0.0, 0.0));
                    keep = clamp((length(beyond) - clearRadius) / max(1.0, clearFeather),
                                 0.0, 1.0);
                }

                lowp float grey = face.r * ink * keep * qt_Opacity;
                lowp float lit = face.g * litInk * keep * qt_Opacity;
                gl_FragColor = vec4(tint.rgb, 1.0) * grey + vec4(litTint.rgb, 1.0) * lit;
            }"
    }
}
