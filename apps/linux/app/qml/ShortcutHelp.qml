import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Copyable hotkey binds per desktop. fsync never grabs keys itself: the desktop runs the command,
// which reaches the running app. The text comes from the engine (same source as `--help`).
ColumnLayout {
    id: help
    spacing: 6
    readonly property var binds: {
        try {
            return JSON.parse(SettingsObject.shortcuts);
        } catch (e) {
            return [];
        }
    }

    Label {
        font.bold: true
        text: qsTr("Keyboard shortcuts")
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#666666"
        text: qsTr("Bind these commands in your desktop. Suggested keys: Super+Alt+C to send, Super+Alt+V to paste (Ctrl+Alt is AltGr on many layouts).")
    }

    Repeater {
        model: help.binds
        delegate: ColumnLayout {
            id: bind
            required property var modelData
            Layout.fillWidth: true
            spacing: 4
            Label {
                font.bold: true
                textFormat: Text.PlainText
                text: bind.modelData.desktop
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: "#666666"
                text: bind.modelData.how
            }
            Repeater {
                model: bind.modelData.lines
                delegate: RowLayout {
                    id: line
                    required property string modelData
                    Layout.fillWidth: true
                    TextField {
                        id: field
                        Layout.fillWidth: true
                        readOnly: true
                        selectByMouse: true
                        font.family: "monospace"
                        text: line.modelData
                    }
                    Button {
                        text: qsTr("Copy")
                        onClicked: {
                            field.selectAll();
                            field.copy();
                            field.deselect();
                        }
                    }
                }
            }
        }
    }
}
