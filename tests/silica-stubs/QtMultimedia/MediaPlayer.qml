import QtQuick 2.0

// QtMultimedia's player, for video. Stubbed for the same reason as Audio:
// a headless runner has no media backend, and the real one would make
// every test that opens a video page depend on the machine's codecs.
// `playbackState` carries QMediaPlayer's own values -- 0 stopped, 1
// playing, 2 paused -- because the real enum lives on the type and a QML
// component cannot declare one on Qt 5.6.
Item {
    property url source
    property bool autoPlay: false
    property int playbackState: 0
    property int position: 0
    property int duration: 0

    function play() { playbackState = 1 }
    function pause() { playbackState = 2 }
    function stop() { playbackState = 0; position = 0 }
    function seek(offset) { position = offset }
}
