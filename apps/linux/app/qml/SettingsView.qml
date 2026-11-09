import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Settings: clipboard status and form, shortcut help, account, autostart, sign out
// (Quit lives in the window footer).
ScrollView {
    id: view
    contentWidth: availableWidth
    clip: true
    // Shown (the tab or the page) -> read the stored values again.
    onVisibleChanged: if (visible) SettingsObject.load()
    Component.onCompleted: if (visible) SettingsObject.load()

    ColumnLayout {
        width: view.availableWidth
        spacing: 12

        Label {
            Layout.margins: 16
            Layout.bottomMargin: 0
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            color: "#664d03"
            visible: SettingsObject.secretsWarning !== ""
            text: SettingsObject.secretsWarning
        }

        ClipboardStatus {
            Layout.margins: 16
            Layout.fillWidth: true
        }
        SettingsClipboardForm {
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.fillWidth: true
        }
        ShortcutHelp {
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.fillWidth: true
        }

        Label {
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.fillWidth: true
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: qsTr("Signed in as %1").arg(EngineObject.email)
        }
        TextField {
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.fillWidth: true
            readOnly: true
            selectByMouse: true
            text: EngineObject.nodeId
        }
        Switch {
            Layout.leftMargin: 16
            text: qsTr("Start at login")
            checked: EngineObject.autostart
            onToggled: EngineObject.setAutostartEnabled(checked)
        }
        Label {
            textFormat: Text.PlainText
            Layout.leftMargin: 16
            Layout.rightMargin: 16
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: !EngineObject.trayAvailable
            color: "#666666"
            text: qsTr("No system tray found: closing the window minimizes it.")
        }
        Button {
            Layout.leftMargin: 16
            Layout.bottomMargin: 16
            text: qsTr("Sign out")
            enabled: !EngineObject.busy
            onClicked: EngineObject.logout()
        }
    }
}
