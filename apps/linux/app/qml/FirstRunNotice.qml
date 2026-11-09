import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Shown once on Wayland: the sensitive-content filter can only see privacy hints there.
Rectangle {
    Layout.fillWidth: true
    implicitHeight: noticeRow.implicitHeight + 16
    color: "#cff4fc"
    visible: ClipboardObject.showNotice

    RowLayout {
        id: noticeRow
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8
        Label {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: "#055160"
            text: qsTr("On Wayland, password managers without a privacy hint may be synced; "
                       + "use Manual mode or the denylist. %1").arg(ClipboardObject.capability)
        }
        Button {
            text: qsTr("Got it")
            onClicked: ClipboardObject.dismissNotice()
        }
    }
}
