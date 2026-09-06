import QtQuick 2.0

// Sailfish's Gecko-backed view, which only a release shipping the browser
// engine has. What this app asks of it is where to go, whether it has
// arrived, which link was tapped, and what the frame script it loaded has
// to say; everything else about it is the browser's business. Nothing
// here loads anything -- that a page is really fetched and drawn is a
// device question, and this cannot answer it.
//
// `active` is deliberately absent from what the app sets: Silica's own
// WebView.qml binds it, and overriding that left a view that never
// rendered (docs/PROJECT.md).
Item {
    property url url
    property bool active: false
    property bool loading: false
    property bool loaded: false
    property int loadProgress: 0
    property string title: ""
    property bool canGoBack: false

    // What was registered and what was loaded, which the real view keeps
    // to itself. A page that forgets either gets no messages from its
    // frame script and no way to know why.
    property string listeners: ""
    property string frameScripts: ""

    // Raised when a link is tapped, before the engine decides what the
    // link is -- which for a file it would rather download is the only
    // moment an app hears about it at all.
    signal linkClicked(string url)

    // A message from the engine's side: what a frame script sends back.
    signal recvAsyncMessage(string message, var data)

    function addMessageListener(name) { listeners += name + ";" }
    function loadFrameScript(where) { frameScripts += where + ";" }
    function sendAsyncMessage(name, data) {}

    function reload() {}
    function stop() {}
    function goBack() {}
}
