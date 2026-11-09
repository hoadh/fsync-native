import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Another device re-created the account's trust list (ADR 012). Accept only if you did that.
Dialog {
    modal: true
    closePolicy: Popup.NoAutoClose
    // Centered in the parent item (Overlay.overlay needs the non-Basic Controls import on Qt 6.2).
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round((parent.height - height) / 2) : 0
    width: parent ? Math.min(440, parent.width - 32) : 440
    title: qsTr("Trust reset requested")
    visible: EngineObject.trustResetId !== ""

    ColumnLayout {
        anchors.fill: parent
        spacing: 12
        Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("A device of your account re-created the list of trusted devices. Accept only if you did this yourself.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("Devices in the new list: %1").arg(EngineObject.trustResetNames)
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            Button {
                text: qsTr("Decline")
                onClicked: EngineObject.declineTrustReset()
            }
            Button {
                text: qsTr("Accept")
                highlighted: true
                onClicked: EngineObject.confirmTrustReset()
            }
        }
    }
}
