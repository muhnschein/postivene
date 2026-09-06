import QtQuick 2.0

// One shared thing: a file by path, or a piece of text.
//
// Never created by an app -- the platform hands the resources over -- and
// the app tells the two kinds apart by which of these carries anything,
// since the real type's enum names cannot be stood in for here (QML
// refuses a property whose name begins with a capital). A test hands the
// provider plain objects of this shape.
QtObject {
    property int type: 0
    property string filePath: ""
    property string data: ""
    property string status: ""
    property string linkTitle: ""
}
