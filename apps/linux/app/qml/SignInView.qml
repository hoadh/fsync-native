import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Email, then the code from the email.
ColumnLayout {
    id: view
    readonly property bool awaitingCode: EngineObject.state === "awaiting-code"
    spacing: 12

    Item { Layout.fillHeight: true }

    Label {
        textFormat: Text.PlainText
        Layout.alignment: Qt.AlignHCenter
        font.pixelSize: 22
        text: qsTr("Sign in to fsync")
    }

    TextField {
        id: emailField
        Layout.alignment: Qt.AlignHCenter
        Layout.preferredWidth: 300
        visible: !view.awaitingCode
        enabled: !EngineObject.busy
        placeholderText: qsTr("Email address")
        inputMethodHints: Qt.ImhEmailCharactersOnly
        onAccepted: sendButton.clicked()
    }
    Button {
        id: sendButton
        Layout.alignment: Qt.AlignHCenter
        visible: !view.awaitingCode
        enabled: !EngineObject.busy
        text: qsTr("Send code")
        onClicked: EngineObject.loginStart(emailField.text)
    }

    Label {
        textFormat: Text.PlainText
        Layout.alignment: Qt.AlignHCenter
        visible: view.awaitingCode
        text: qsTr("Enter the code sent to %1").arg(EngineObject.email)
    }
    TextField {
        id: codeField
        Layout.alignment: Qt.AlignHCenter
        Layout.preferredWidth: 200
        visible: view.awaitingCode
        enabled: !EngineObject.busy
        placeholderText: qsTr("Code")
        inputMethodHints: Qt.ImhDigitsOnly
        onAccepted: verifyButton.clicked()
    }
    RowLayout {
        Layout.alignment: Qt.AlignHCenter
        visible: view.awaitingCode
        Button {
            text: qsTr("Back")
            enabled: !EngineObject.busy
            onClicked: {
                codeField.text = "";
                EngineObject.backToEmail();
            }
        }
        Button {
            id: verifyButton
            text: qsTr("Sign in")
            enabled: !EngineObject.busy
            onClicked: EngineObject.loginVerify(codeField.text)
        }
    }

    BusyIndicator {
        Layout.alignment: Qt.AlignHCenter
        running: EngineObject.busy
    }

    Item { Layout.fillHeight: true }
}
