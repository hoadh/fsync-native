import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Main window. `smoke` and `startHidden` are initial properties set by main.rs.
ApplicationWindow {
    id: root
    property bool smoke: false
    property bool startHidden: false
    // Index into the page stack, derived from the engine state.
    readonly property int page: {
        switch (EngineObject.state) {
        case "starting":
        case "waiting-for-keyring":
            return 0;
        case "signed-out":
        case "awaiting-code":
            return 1;
        case "ready":
        case "needs-pairing":
        case "offline":
            return 2;
        default:
            return 3;
        }
    }

    width: 520
    height: 680
    minimumWidth: 420
    minimumHeight: 480
    visible: !startHidden
    title: qsTr("fsync")

    // Set before Qt.quit(): quit() closes every window first, and our close handler refuses.
    property bool quitting: false

    function quitNow() {
        root.quitting = true;
        Qt.quit();
    }

    // Runs synchronously inside EngineObject.showWindowRequested, which puts the launcher's
    // xdg-activation token in XDG_ACTIVATION_TOKEN for exactly this call (Qt >= 6.3 on Wayland
    // consumes it in requestActivate; Qt 6.2 and X11 ignore it).
    function raiseWindow() {
        root.show();
        root.raise();
        root.requestActivate();
    }

    // Quit stays reachable on every page (a GNOME session without a tray has no other way).
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            Item { Layout.fillWidth: true }
            ToolButton {
                text: qsTr("Quit fsync")
                onClicked: EngineObject.requestQuit()
            }
        }
    }

    Connections {
        target: EngineObject
        function onShowWindowRequested(token) {
            root.raiseWindow();
        }
        function onSectionRequested(section) {
            if (section === "history")
                tabs.currentIndex = 2;
            else if (section === "offers")
                tabs.currentIndex = 3;
            else if (section === "send")
                devicePicker.open();
        }
        function onToggleWindowRequested() {
            if (root.visible && root.active)
                root.hide();
            else
                root.raiseWindow();
        }
        function onQuitRequested() {
            root.quitNow();
        }
    }

    // Close hides to the tray when one exists, else minimizes: the window stays reachable.
    onClosing: function (close) {
        if (root.quitting)
            return;
        close.accepted = false;
        if (EngineObject.windowCloseHides())
            root.hide();
        else
            root.showMinimized();
    }

    // Files dropped anywhere on the signed-in window open the DevicePicker (a drop on a device
    // row is taken by that row first: it is stacked above).
    SendDropArea {
        anchors.fill: parent
        enabled: root.page === 2
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        StatusBanner {
            Layout.fillWidth: true
        }

        FirstRunNotice {
            Layout.fillWidth: true
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.page

            // 0: starting / waiting for the keyring
            ColumnLayout {
                Item { Layout.fillHeight: true }
                BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    running: root.page === 0
                }
                Label {
                    textFormat: Text.PlainText
                    Layout.alignment: Qt.AlignHCenter
                    text: EngineObject.state === "waiting-for-keyring"
                          ? qsTr("Waiting for the system keyring...") : qsTr("Starting...")
                }
                Button {
                    Layout.alignment: Qt.AlignHCenter
                    visible: EngineObject.state === "waiting-for-keyring"
                    text: qsTr("Unlock keyring")
                    onClicked: EngineObject.unlockKeyring()
                }
                Item { Layout.fillHeight: true }
            }

            // 1: sign in
            SignInView {}

            // 2: signed in
            ColumnLayout {
                spacing: 0
                TabBar {
                    id: tabs
                    Layout.fillWidth: true
                    TabButton { text: qsTr("Devices") }
                    TabButton { text: qsTr("Pair") }
                    TabButton { text: qsTr("History") }
                    TabButton {
                        text: TransfersObject.pendingCount > 0
                              ? qsTr("Offers (%1)").arg(TransfersObject.pendingCount) : qsTr("Offers")
                    }
                    TabButton { text: qsTr("Settings") }
                }
                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: tabs.currentIndex
                    DevicesView {}
                    PairingView {}
                    HistoryView {}
                    OffersView {}
                    SettingsView {}
                }
            }

            // 3: error
            ErrorView {
                maxTextWidth: root.width - 48
                onResetHistoryRequested: resetHistoryDialog.open()
            }
        }
    }

    // Manual clipboard path (GNOME): opened by ClipboardObject.quickClipRequested.
    QuickClipWindow { id: quickClip }
    Connections {
        target: ClipboardObject
        function onQuickClipRequested(action, token) {
            quickClip.begin(action);
        }
    }

    DevicePicker { id: devicePicker }
    SasDialog {}
    TrustResetDialog {}
    ResetHistoryDialog { id: resetHistoryDialog }

    // After a failed pairing/offline state the user may need to retry from the tabs.
    onPageChanged: {
        if (page !== 2)
            devicePicker.close();
        if (page === 2)
            tabs.currentIndex = (EngineObject.state === "needs-pairing") ? 1 : 0;
    }

    Component.onCompleted: {
        // Smoke: all pages and dialogs above are instantiated; one event-loop turn, then quit.
        if (root.smoke)
            Qt.callLater(root.quitNow);
    }
}
