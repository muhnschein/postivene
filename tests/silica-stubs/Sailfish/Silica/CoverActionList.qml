import QtQuick 2.0
// The real one is attached to a cover; here it only has to hold its
// children so a cover using one still loads.
QtObject {
    default property list<QtObject> actions
}
