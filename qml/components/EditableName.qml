import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A name under a picture, and the way to change it.
 *
 * Drawn as a name: centred, in the highlight colour, with an edit badge
 * at its top right -- the same badge the picture above it wears. A tap
 * on the badge turns the name into a field, centred where the name was,
 * with a line under it saying what the field means; the badge turns into
 * a tick, and a tap on that puts the name back. Leaving the page puts it
 * back too (the page does that), so a name left blank is a name again
 * the next time the page is seen, not an empty field.
 *
 * The field is what the page reads and writes: `text` is its text, so
 * the page fills it from the core and saves what is typed the way it did
 * when the field stood on its own. What the name shows is the text, or
 * `fallbackText` when there is none -- the name a contact chose for
 * themselves -- or the placeholder, dimmed, when there is neither.
 *
 * Used for a contact's name on their page, the reader's own on the
 * profile page and a group's on the group page. Each names the inner
 * items for its tests through the *ObjectName properties, the way Banner
 * does.
 */
Item {
    id: root

    /// The field's text: the name given here.
    property alias text: field.text
    /// Shown as the name when the text is empty. For a contact, the name
    /// they chose for themselves.
    property string fallbackText: ""
    /// The field's placeholder, and what the name shows, dimmed, when
    /// there is neither text nor fallback.
    property string placeholderText: ""
    /// Said under the field while editing, and only then.
    property string hint: ""
    /// Whether the badge is offered at all.
    property bool canEdit: true
    /// The field is up. The badge sets and clears it; the page clears it
    /// on the way out.
    property bool editing: false

    property alias labelObjectName: nameLabel.objectName
    property alias fieldObjectName: field.objectName
    property alias badgeObjectName: badge.objectName
    property alias hintObjectName: hintLabel.objectName

    /// What the name shows: the text, their own, or the placeholder.
    readonly property string shownText:
        root.text.length > 0 ? root.text
                             : root.fallbackText.length > 0 ? root.fallbackText
                                                            : root.placeholderText
    /// The placeholder is standing in, so it is drawn dimmed.
    readonly property bool showingPlaceholder:
        root.text.length === 0 && root.fallbackText.length === 0

    width: parent ? parent.width : 0
    height: root.editing ? editor.height : nameLabel.height

    // Opening the field puts the cursor in it; closing it drops the
    // keyboard with it.
    onEditingChanged: {
        if (root.editing) {
            field.forceActiveFocus()
        } else if (field.activeFocus) {
            field.focus = false
        }
    }

    // The name. Its width is its own, up to the room left beside the
    // badge, so the badge sits at the end of the text rather than at
    // the edge of the page.
    Label {
        id: nameLabel
        visible: !root.editing
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(implicitWidth,
                        root.width - 2 * Theme.horizontalPageMargin
                        - badge.width - Theme.paddingSmall)
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        // A name is whatever the other end chose, so it is drawn as
        // written.
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeLarge
        color: root.showingPlaceholder ? Theme.secondaryHighlightColor
                                       : Theme.highlightColor
        text: root.shownText
    }

    // The field, where the name was. A field draws what it holds as
    // text and nothing else, so it needs no pinning to plain.
    Column {
        id: editor
        visible: root.editing
        width: parent.width

        TextField {
            id: field
            width: parent.width
            horizontalAlignment: TextInput.AlignHCenter
            placeholderText: root.placeholderText
            readOnly: !root.canEdit
            // No label under the text: the hint below says what the
            // field is, and only while it is up.
            labelVisible: false
        }

        Label {
            id: hintLabel
            visible: root.hint.length > 0
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: root.hint
        }
    }

    // The badge: at the top right of the name, or of the field. The same
    // round badge the picture wears, a pencil to open and a tick to close.
    Rectangle {
        id: badge
        visible: root.canEdit
        anchors {
            left: root.editing ? undefined : nameLabel.right
            right: root.editing ? field.right : undefined
            rightMargin: Theme.horizontalPageMargin
            leftMargin: Theme.paddingSmall
            verticalCenter: root.editing ? field.top : nameLabel.top
            verticalCenterOffset: root.editing ? badge.height / 2 : 0
        }
        width: Theme.iconSizeMedium
        height: width
        radius: width / 2
        color: root.editing ? Theme.highlightColor : Theme.highlightBackgroundColor

        Image {
            anchors.centerIn: parent
            // The medium tick drawn at the small size: there is no small
            // one, and a pencil is not a way to say "done".
            width: Theme.iconSizeSmall
            height: width
            fillMode: Image.PreserveAspectFit
            source: root.editing ? "image://theme/icon-m-accept"
                                 : "image://theme/icon-s-edit"
        }
    }

    // The name and its badge open the field; the badge alone closes it,
    // so a tap on the field is a tap in the field.
    MouseArea {
        objectName: "nameTap"
        visible: !root.editing && root.canEdit
        anchors {
            left: nameLabel.left
            right: badge.right
            top: badge.top
            bottom: nameLabel.bottom
        }
        onClicked: root.editing = true
    }

    MouseArea {
        objectName: "doneTap"
        visible: root.editing
        anchors.fill: badge
        anchors.margins: -Theme.paddingMedium
        onClicked: root.editing = false
    }
}
