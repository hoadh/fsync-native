import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Short authentication string: both devices must show the same three words.
Dialog {
    id: dialog
    modal: true
    closePolicy: Popup.NoAutoClose
    // Centered in the parent item (Overlay.overlay needs the non-Basic Controls import on Qt 6.2).
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round((parent.height - height) / 2) : 0
    width: parent ? Math.min(400, parent.width - 32) : 400
    title: qsTr("Do the words match?")
    visible: PairingObject.phase === "confirm"
    // The id the user is looking at; the answer is only valid for this confirmation.
    readonly property string shownRequestId: PairingObject.requestId

    ColumnLayout {
        anchors.fill: parent
        spacing: 12
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("Compare with %1. Accept only if both devices show exactly these words.").arg(PairingObject.peerName)
        }
        Label {
            Layout.alignment: Qt.AlignHCenter
            font.pixelSize: 24
            font.bold: true
            textFormat: Text.PlainText
            text: PairingObject.sas
        }
        Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: PairingObject.message !== ""
            text: PairingObject.message
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            Button {
                text: qsTr("They differ")
                onClicked: PairingObject.confirm(dialog.shownRequestId, false)
            }
            Button {
                text: qsTr("They match")
                highlighted: true
                onClicked: PairingObject.confirm(dialog.shownRequestId, true)
            }
        }
    }
}
