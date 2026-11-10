import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// The stored keys do not open the clipboard history. Resetting moves the old history aside
// (nothing is deleted) and this device must pair again.
Dialog {
    id: dialog
    modal: true
    // Centered in the parent item (Overlay.overlay needs the non-Basic Controls import on Qt 6.2).
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round((parent.height - height) / 2) : 0
    width: parent ? Math.min(440, parent.width - 32) : 440
    title: qsTr("Reset clipboard history?")
    footer: DialogButtonBox {
        Button {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        Button {
            text: qsTr("Reset history")
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
    }
    onAccepted: EngineObject.resetHistory()

    ColumnLayout {
        anchors.fill: parent
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("The old clipboard history cannot be opened with this device's keys. It is moved aside, not deleted. A new history is started and this device has to be paired again.")
        }
    }
}
