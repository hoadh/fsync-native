import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Engine error page: the message, "Try again" and (after a key mismatch) "Reset history...".
ColumnLayout {
    id: view
    property real maxTextWidth: 400
    signal resetHistoryRequested()

    Item { Layout.fillHeight: true }
    Label {
        textFormat: Text.PlainText
        Layout.alignment: Qt.AlignHCenter
        Layout.maximumWidth: view.maxTextWidth
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        text: EngineObject.message
    }
    Button {
        Layout.alignment: Qt.AlignHCenter
        text: qsTr("Try again")
        enabled: !EngineObject.busy
        onClicked: EngineObject.retry()
    }
    Button {
        Layout.alignment: Qt.AlignHCenter
        visible: EngineObject.canResetHistory
        enabled: !EngineObject.busy
        text: qsTr("Reset history...")
        onClicked: view.resetHistoryRequested()
    }
    Item { Layout.fillHeight: true }
}
