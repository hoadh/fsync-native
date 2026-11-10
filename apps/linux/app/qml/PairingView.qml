import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Pair another device: show a code or QR here, or join with a code / QR text from the other one.
ColumnLayout {
    id: view
    readonly property string phase: PairingObject.phase
    // Set on click, cleared when the phase moves on: a double click cannot start two offers.
    property bool starting: false
    onPhaseChanged: starting = false
    spacing: 12

    Label {
        textFormat: Text.PlainText
        Layout.margins: 16
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        visible: EngineObject.state === "needs-pairing" && view.phase === "idle"
        text: qsTr("This device is not paired yet. Show a code here and enter it on a device that is already signed in.")
    }

    // Idle: choose.
    ColumnLayout {
        Layout.margins: 16
        Layout.fillWidth: true
        visible: view.phase === "idle"
        spacing: 12
        Label { font.bold: true; text: qsTr("Show on this device") }
        RowLayout {
            Button {
                text: qsTr("Show code")
                enabled: !view.starting
                onClicked: { view.starting = true; PairingObject.offerCode(); }
            }
            Button {
                text: qsTr("Show QR")
                enabled: !view.starting
                onClicked: { view.starting = true; PairingObject.offerQr(); }
            }
        }
        Label { font.bold: true; text: qsTr("Join another device") }
        RowLayout {
            Layout.fillWidth: true
            TextField {
                id: joinField
                Layout.fillWidth: true
                placeholderText: qsTr("Code or QR text")
                onAccepted: joinButton.clicked()
            }
            Button {
                id: joinButton
                text: qsTr("Join")
                enabled: !view.starting
                onClicked: { view.starting = true; PairingObject.join(joinField.text); }
            }
        }
        Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: PairingObject.message !== ""
            text: PairingObject.message
        }
    }

    // Offering: code and/or QR.
    ColumnLayout {
        Layout.margins: 16
        Layout.fillWidth: true
        visible: view.phase === "offering"
        spacing: 12
        BusyIndicator {
            Layout.alignment: Qt.AlignHCenter
            running: PairingObject.code === "" && PairingObject.qrUrl === ""
        }
        Label {
            textFormat: Text.PlainText
            Layout.alignment: Qt.AlignHCenter
            visible: PairingObject.code !== ""
            font.pixelSize: 36
            font.family: "monospace"
            text: PairingObject.code
        }
        Image {
            Layout.alignment: Qt.AlignHCenter
            visible: PairingObject.qrUrl !== ""
            source: PairingObject.qrUrl
            fillMode: Image.PreserveAspectFit
            Layout.preferredWidth: 240
            Layout.preferredHeight: 240
            smooth: false
        }
        TextField {
            Layout.fillWidth: true
            visible: PairingObject.qrText !== ""
            readOnly: true
            selectByMouse: true
            text: PairingObject.qrText
        }
        Label {
            textFormat: Text.PlainText
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Waiting for the other device...")
        }
        Button {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Cancel")
            onClicked: PairingObject.cancel()
        }
    }

    // Joining or waiting after the SAS answer.
    ColumnLayout {
        Layout.margins: 16
        Layout.fillWidth: true
        visible: view.phase === "joining" || view.phase === "confirm" || view.phase === "answering"
        spacing: 12
        BusyIndicator { Layout.alignment: Qt.AlignHCenter; running: parent.visible }
        Label {
            textFormat: Text.PlainText
            Layout.alignment: Qt.AlignHCenter
            text: view.phase === "joining" ? qsTr("Connecting...")
                  : view.phase === "answering" ? qsTr("Waiting for the other device...")
                  : qsTr("Compare the words...")
        }
        Button {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Cancel")
            onClicked: PairingObject.cancel()
        }
    }

    // Done / failed.
    ColumnLayout {
        Layout.margins: 16
        Layout.fillWidth: true
        visible: view.phase === "done" || view.phase === "failed"
        spacing: 12
        Label {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            text: view.phase === "done" ? qsTr("Paired.") : PairingObject.message
        }
        Button {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("OK")
            onClicked: PairingObject.reset()
        }
    }

    Item { Layout.fillHeight: true }
}
