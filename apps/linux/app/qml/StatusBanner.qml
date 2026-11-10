import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Persistent warnings (keyring, unsaved secrets) and the last action message.
ColumnLayout {
    spacing: 0
    visible: EngineObject.banner !== "" || (EngineObject.message !== "" && EngineObject.state !== "error")

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: bannerRow.implicitHeight + 16
        color: "#fff3cd"
        visible: EngineObject.banner !== ""
        RowLayout {
            id: bannerRow
            anchors.fill: parent
            anchors.margins: 8
            spacing: 8
            Label {
                id: bannerText
                Layout.fillWidth: true
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: "#664d03"
                text: EngineObject.banner
            }
            // Keyring locked after start (or a save that could not reach it): one click asks
            // for one unlock prompt and retries the save.
            Button {
                visible: EngineObject.canUnlockKeyring
                text: qsTr("Unlock keyring")
                onClicked: EngineObject.unlockKeyring()
            }
        }
    }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: messageText.implicitHeight + 16
        color: "#f8d7da"
        visible: EngineObject.message !== "" && EngineObject.state !== "error"
        Label {
            textFormat: Text.PlainText
            id: messageText
            anchors.fill: parent
            anchors.margins: 8
            wrapMode: Text.Wrap
            color: "#842029"
            text: EngineObject.message
        }
    }
}
