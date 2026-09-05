import QtQuick 2.0

// What a page writes into the files the camera makes. The page's grouped
// `metaData.orientation` binding resolves against the property here, and
// a test reads it back.
QtObject {
    property var orientation
}
