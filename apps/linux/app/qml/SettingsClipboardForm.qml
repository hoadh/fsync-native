import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Editable core settings: clip mode, retention, size limit, file auto-accept, app denylist.
// Writes go through core validation (`SettingsObject.save`); the screen shows what was stored.
ColumnLayout {
    id: form
    spacing: 8
    enabled: SettingsObject.loaded && !SettingsObject.busy

    function save() {
        SettingsObject.save(mode.currentIndex === 1, days.value, retention.value, maxClip.value,
                            autoAccept.checked, deny.text);
    }

    // Reloads every control from the stored values (after load / save: a binding would break
    // as soon as the user edits).
    Connections {
        target: SettingsObject
        function onLoadedChanged() { form.pull(); }
        function onManualChanged() { form.pull(); }
        function onRetentionDaysChanged() { form.pull(); }
        function onRetentionMibChanged() { form.pull(); }
        function onMaxClipMibChanged() { form.pull(); }
        function onAutoAcceptChanged() { form.pull(); }
        function onDenylistChanged() { form.pull(); }
    }
    function pull() {
        mode.currentIndex = SettingsObject.manual ? 1 : 0;
        days.value = SettingsObject.retentionDays;
        retention.value = SettingsObject.retentionMib;
        maxClip.value = SettingsObject.maxClipMib;
        autoAccept.checked = SettingsObject.autoAccept;
        deny.text = SettingsObject.denylist;
    }
    Component.onCompleted: pull()

    Label {
        font.bold: true
        text: qsTr("Sync and storage")
    }
    RowLayout {
        Label { text: qsTr("Received clips") }
        ComboBox {
            id: mode
            model: [qsTr("Apply automatically"), qsTr("Ask me (manual paste)")]
        }
    }
    RowLayout {
        Label { text: qsTr("Keep for") }
        SpinBox { id: days; from: 1; to: 3650; editable: true }
        Label { text: qsTr("days") }
    }
    RowLayout {
        Label { text: qsTr("Keep at most") }
        SpinBox { id: retention; from: 1; to: 2147483647; editable: true }
        Label { text: qsTr("MiB") }
    }
    RowLayout {
        Label { text: qsTr("Largest clip to send") }
        SpinBox { id: maxClip; from: 1; to: 2147483647; editable: true }
        Label { text: qsTr("MiB") }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: qsTr("Days and size apply to the clipboard history and to the folder of received files. Large file clips you download yourself are removed by age only, they do not count toward the size limit.")
    }
    Switch {
        id: autoAccept
        text: qsTr("Accept files sent by my devices without asking")
    }

    Label {
        font.bold: true
        text: qsTr("Apps whose copies are never synced")
    }
    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: 90
        TextArea {
            id: deny
            placeholderText: qsTr("One app per line")
            wrapMode: TextEdit.NoWrap
            selectByMouse: true
        }
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: qsTr("Use the process name on X11 and the app id on Hyprland and Sway. On other desktops the app cannot be told, only password-manager hints are honoured. %1").arg(ClipboardObject.capability)
    }

    RowLayout {
        Button {
            text: qsTr("Save")
            onClicked: form.save()
        }
        BusyIndicator {
            running: SettingsObject.busy
            visible: running
            implicitHeight: 24
            implicitWidth: 24
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            visible: SettingsObject.status !== ""
            color: SettingsObject.statusIsError ? "#842029" : "#198754"
            text: SettingsObject.status
        }
    }
}
