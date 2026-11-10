import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Clipboard backend, mode, held clip and the explicit send / paste / pause actions.
ColumnLayout {
    spacing: 6

    Label {
        font.bold: true
        text: qsTr("Clipboard")
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: qsTr("%1 - %2").arg(ClipboardObject.backendName)
                .arg(ClipboardObject.autoSync ? qsTr("automatic sync") : qsTr("manual"))
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: ClipboardObject.reason
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: ClipboardObject.capability
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        visible: ClipboardObject.pendingText !== ""
        text: qsTr("Waiting to paste: %1").arg(ClipboardObject.pendingText)
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        visible: ClipboardObject.lastEvent !== ""
        text: ClipboardObject.lastEvent
    }
    RowLayout {
        spacing: 8
        Button {
            text: qsTr("Send clipboard now")
            onClicked: ClipboardObject.sendNow()
        }
        Button {
            text: qsTr("Paste received clip")
            enabled: ClipboardObject.pendingText !== ""
            onClicked: ClipboardObject.pasteNow()
        }
    }
    Switch {
        id: pauseSwitch
        text: qsTr("Pause sync")
        onToggled: ClipboardObject.togglePause()
    }
    // A user toggle breaks a plain `checked:` binding; this one survives it.
    Binding {
        target: pauseSwitch
        property: "checked"
        value: ClipboardObject.syncPaused
    }
}
