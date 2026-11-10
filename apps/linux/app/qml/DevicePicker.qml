import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// "Send to...": opened by `fsync-linux --send <files>` (and by a drop on the window). The files
// are already staged and checked by the engine; the summary below is what a click sends. A new
// set staged while the picker is open replaces the summary and says so; an emptied set (sign-out,
// cancel) closes it. Cancel / Escape forget the set.
Dialog {
    id: dialog
    modal: true
    // Only Escape and Cancel close it: a click outside must not drop the set unseen.
    closePolicy: Popup.CloseOnEscape
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round((parent.height - height) / 2) : 0
    width: parent ? Math.min(420, parent.width - 32) : 420
    height: parent ? Math.min(460, parent.height - 32) : 460
    title: qsTr("Send to which device?")
    standardButtons: Dialog.Cancel
    onRejected: SendObject.cancelPick()

    // Set when the shown set changed while the picker was open.
    property bool changedMeanwhile: false
    onOpened: changedMeanwhile = false

    Connections {
        target: SendObject
        function onStageIdChanged() {
            if (SendObject.stageId === "")
                dialog.close();
            else if (dialog.opened)
                dialog.changedMeanwhile = true;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font.bold: true
            text: SendObject.summary
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            color: "#842029"
            visible: dialog.changedMeanwhile
            text: qsTr("The files changed while this window was open. Check the list above.")
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: DevicesModel
            delegate: ItemDelegate {
                id: row
                required property string name
                required property string platform
                required property string status
                required property string nodeId
                required property bool isSelf
                width: list.width
                height: row.isSelf ? 0 : implicitHeight
                visible: !row.isSelf
                text: row.status === "Offline" ? qsTr("%1 (offline)").arg(row.name) : row.name
                onClicked: {
                    // Answers the set on display; the engine refuses it if another one was
                    // staged since.
                    SendObject.pickDevice(row.nodeId, SendObject.stageId);
                    dialog.accept();
                }
            }
            Label {
                textFormat: Text.PlainText
                anchors.centerIn: parent
                visible: DevicesModel.count <= 1
                text: qsTr("No other paired device.")
            }
        }
    }
}
