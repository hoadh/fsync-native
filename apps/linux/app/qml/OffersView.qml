import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Files other devices want to send to this one: accept or decline, progress, cancel, open folder.
ColumnLayout {
    spacing: 0

    function sizeText(bytes) {
        if (bytes < 1024 * 1024)
            return qsTr("%1 KiB").arg((bytes / 1024).toFixed(1));
        if (bytes < 1024 * 1024 * 1024)
            return qsTr("%1 MiB").arg((bytes / 1048576).toFixed(1));
        return qsTr("%1 GiB").arg((bytes / 1073741824).toFixed(2));
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: TransfersObject
        delegate: ColumnLayout {
            id: row
            required property string itemId
            required property string fromName
            required property int fileCount
            required property real totalSize
            required property real doneBytes
            required property string offerState
            required property string message
            required property string dir
            width: list.width
            spacing: 6

            Label {
                Layout.margins: 12
                Layout.bottomMargin: 0
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font.bold: true
                textFormat: Text.PlainText
                text: qsTr("%1 wants to send %n file(s)", "", row.fileCount).arg(row.fromName) + " (" + sizeText(row.totalSize) + ")"
            }
            Label {
                Layout.leftMargin: 12
                Layout.rightMargin: 12
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#666666"
                textFormat: Text.PlainText
                text: {
                    switch (row.offerState) {
                    case "pending": return qsTr("Waiting for your answer");
                    case "receiving": return qsTr("Receiving...");
                    case "done": return qsTr("Saved to %1").arg(row.dir);
                    case "declined": return qsTr("Declined");
                    case "cancelled": return qsTr("Cancelled");
                    default: return row.message !== "" ? qsTr("Failed: %1").arg(row.message) : qsTr("Failed");
                    }
                }
            }
            ProgressBar {
                Layout.leftMargin: 12
                Layout.rightMargin: 12
                Layout.fillWidth: true
                visible: row.offerState === "receiving"
                from: 0
                to: row.totalSize > 0 ? row.totalSize : 1
                value: row.doneBytes
            }
            RowLayout {
                Layout.leftMargin: 12
                Layout.rightMargin: 12
                Layout.bottomMargin: 8
                spacing: 8
                Button {
                    visible: row.offerState === "pending"
                    text: qsTr("Accept")
                    onClicked: TransfersObject.accept(row.itemId)
                }
                Button {
                    visible: row.offerState === "pending"
                    text: qsTr("Decline")
                    onClicked: TransfersObject.decline(row.itemId)
                }
                Button {
                    visible: row.offerState === "receiving"
                    text: qsTr("Cancel")
                    onClicked: TransfersObject.cancel(row.itemId)
                }
                Button {
                    visible: row.offerState === "done"
                    text: qsTr("Open folder")
                    onClicked: TransfersObject.openFolder(row.itemId)
                }
                Button {
                    visible: row.offerState !== "pending" && row.offerState !== "receiving"
                    text: qsTr("Dismiss")
                    onClicked: TransfersObject.dismiss(row.itemId)
                }
            }
            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: "#dddddd"
            }
        }
        Label {
            textFormat: Text.PlainText
            anchors.centerIn: parent
            visible: list.count === 0
            text: qsTr("No files offered. Use \"Send to\" on another device.")
        }
    }
}
