import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Account devices with live status; remove asks for confirmation.
ColumnLayout {
    spacing: 0

    Label {
        textFormat: Text.PlainText
        Layout.margins: 16
        visible: EngineObject.state === "offline"
        wrapMode: Text.Wrap
        Layout.fillWidth: true
        text: qsTr("Offline: the server is not reachable.")
    }
    Button {
        Layout.leftMargin: 16
        visible: EngineObject.state === "offline"
        text: qsTr("Retry")
        onClicked: EngineObject.retry()
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: DevicesModel
        delegate: Item {
            id: row
            required property string name
            required property string platform
            required property string status
            required property string nodeId
            required property bool isSelf
            width: list.width
            implicitHeight: content.implicitHeight
            height: implicitHeight

            RowLayout {
                id: content
                anchors.fill: parent
                spacing: 12
                ColumnLayout {
                    Layout.margins: 12
                    Layout.fillWidth: true
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        font.bold: true
                        text: row.isSelf ? qsTr("%1 (this device)").arg(row.name) : row.name
                    }
                    Label { color: "#666666"; textFormat: Text.PlainText; text: row.platform }
                }
                Label {
                    textFormat: Text.PlainText
                    text: row.isSelf ? qsTr("This device") : row.status === "Lan" ? qsTr("LAN") : row.status === "Relay" ? qsTr("Relay") : qsTr("Offline")
                    color: row.status === "Offline" && !row.isSelf ? "#888888" : "#198754"
                }
                Button {
                    Layout.rightMargin: 12
                    visible: !row.isSelf
                    text: qsTr("Remove")
                    onClicked: {
                        removeDialog.nodeId = row.nodeId;
                        removeDialog.deviceName = row.name;
                        removeDialog.open();
                    }
                }
            }
            // Files dropped on a row go to that device.
            SendDropArea {
                id: rowDrop
                anchors.fill: parent
                enabled: !row.isSelf
                nodeId: row.nodeId
            }
            Rectangle {
                anchors.fill: parent
                color: "#2000a0ff"
                border.color: "#0078d4"
                visible: rowDrop.containsDrag
            }
        }
        Label {
            textFormat: Text.PlainText
            anchors.centerIn: parent
            visible: list.count === 0
            text: qsTr("No devices yet.")
        }
    }

    Label {
        Layout.margins: 12
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: qsTr("Drop files on a device to send them.")
    }

    Dialog {
        id: removeDialog
        property string nodeId: ""
        property string deviceName: ""
        modal: true
        x: parent ? Math.round((parent.width - width) / 2) : 0
        y: parent ? Math.round((parent.height - height) / 2) : 0
        title: qsTr("Remove device?")
        footer: DialogButtonBox {
            Button {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            Button {
                text: qsTr("Remove")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: DevicesModel.removeDevice(removeDialog.nodeId)
        Label {
            textFormat: Text.PlainText
            text: qsTr("%1 will no longer sync with this account.").arg(removeDialog.deviceName)
        }
    }
}
