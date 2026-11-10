import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Clipboard history: search, copy back, pin, delete, clear (keeps pinned), download large file clips.
ColumnLayout {
    id: view
    spacing: 0

    function kindGlyph(kind) {
        switch (kind) {
        case "text": return "Aa";
        case "rich text": return "<>";
        case "image": return "IMG";
        default: return "FILE";
        }
    }

    function sizeText(bytes) {
        if (bytes < 1024)
            return qsTr("%1 B").arg(bytes);
        if (bytes < 1024 * 1024)
            return qsTr("%1 KiB").arg((bytes / 1024).toFixed(1));
        return qsTr("%1 MiB").arg((bytes / 1048576).toFixed(1));
    }

    // Reloads when the tab is shown and when something was sent or received meanwhile.
    onVisibleChanged: if (visible) HistoryModel.refresh()
    Component.onCompleted: if (visible) HistoryModel.refresh()
    Connections {
        target: ClipboardObject
        function onLastEventChanged() {
            if (view.visible)
                HistoryModel.refresh();
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: 12
        TextField {
            id: searchField
            Layout.fillWidth: true
            placeholderText: qsTr("Search history")
            onTextChanged: debounce.restart()
        }
        Button {
            text: qsTr("Clear")
            enabled: list.count > 0
            onClicked: clearDialog.open()
        }
    }
    // Search runs 250 ms after the last keystroke.
    Timer {
        id: debounce
        interval: 250
        onTriggered: HistoryModel.search(searchField.text)
    }

    Label {
        Layout.leftMargin: 12
        Layout.rightMargin: 12
        Layout.fillWidth: true
        visible: HistoryModel.message !== ""
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: "#444444"
        text: HistoryModel.message
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: HistoryModel
        delegate: ColumnLayout {
            id: row
            required property string itemId
            required property string kind
            required property string preview
            required property string peerName
            required property real createdMs
            required property real totalSize
            required property bool pinned
            required property bool outgoing
            required property bool canCopy
            required property bool canDownload
            required property bool downloading
            width: list.width
            spacing: 4

            RowLayout {
                Layout.leftMargin: 12
                Layout.rightMargin: 12
                Layout.topMargin: 8
                spacing: 10
                Label {
                    Layout.preferredWidth: 40
                    horizontalAlignment: Text.AlignHCenter
                    font.bold: true
                    color: "#666666"
                    textFormat: Text.PlainText
                    text: view.kindGlyph(row.kind)
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: row.preview !== "" ? row.preview : qsTr("(%1, %2)").arg(row.kind).arg(view.sizeText(row.totalSize))
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        color: "#666666"
                        font.pixelSize: 11
                        textFormat: Text.PlainText
                        text: (row.outgoing ? qsTr("To other devices") : qsTr("From %1").arg(row.peerName))
                              + " · " + Qt.formatDateTime(new Date(row.createdMs), "yyyy-MM-dd hh:mm")
                              + " · " + view.sizeText(row.totalSize)
                              + (row.pinned ? " · " + qsTr("pinned") : "")
                    }
                }
            }
            RowLayout {
                Layout.leftMargin: 62
                Layout.rightMargin: 12
                Layout.bottomMargin: 6
                spacing: 8
                Button {
                    text: qsTr("Copy")
                    enabled: row.canCopy
                    onClicked: HistoryModel.copy(row.itemId)
                }
                Button {
                    visible: row.canDownload
                    enabled: !row.downloading
                    text: row.downloading ? qsTr("Downloading...") : qsTr("Download")
                    onClicked: HistoryModel.download(row.itemId)
                }
                Button {
                    text: row.pinned ? qsTr("Unpin") : qsTr("Pin")
                    onClicked: HistoryModel.pin(row.itemId, !row.pinned)
                }
                Button {
                    text: qsTr("Delete")
                    onClicked: HistoryModel.removeItem(row.itemId)
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
            visible: list.count === 0 && !HistoryModel.loading
            text: qsTr("Nothing here yet.")
        }
    }

    Dialog {
        id: clearDialog
        modal: true
        x: parent ? Math.round((parent.width - width) / 2) : 0
        y: parent ? Math.round((parent.height - height) / 2) : 0
        title: qsTr("Clear history?")
        footer: DialogButtonBox {
            Button {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            Button {
                text: qsTr("Clear")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: HistoryModel.clearUnpinned()
        Label {
            textFormat: Text.PlainText
            text: qsTr("Pinned items are kept.")
        }
    }
}
