import QtQuick 2.0

// Sailfish's Gecko-backed view, which only a release shipping the browser
// engine has. What this app asks of it is where to go and whether it is
// on; everything else about it is the browser's business. Nothing here
// loads anything -- that a page is really fetched and drawn is a device
// question, and this cannot answer it.
Item {
    property url url
    property bool active: false
    property bool loading: false
    property string title: ""
    property bool canGoBack: false
    function reload() {}
    function stop() {}
    function goBack() {}
}
