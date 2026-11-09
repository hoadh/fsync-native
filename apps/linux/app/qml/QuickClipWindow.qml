import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import app.fsync

// Small focused window for the manual clipboard path (GNOME Wayland has no background
// clipboard access: only a focused surface may read or write it). `begin()` runs inside
// ClipboardObject.quickClipRequested, which puts the launcher's xdg-activation token in the
// environment for exactly that call. The clipboard is only touched once the window is active;
// if the compositor refuses the focus, a click on the button gives it (user input = focus).
Window {
    id: win

    // "send" or "paste"; empty while idle.
    property string action: ""
    property bool ran: false
    property bool needsClick: false
    property string result: ""
    // QuickClip.run spins the event loop (paste settle): a command arriving meanwhile is kept
    // here and handled after the running call returns, never nested.
    property bool busy: false
    property string pendingAction: ""
    property double lastRunAt: 0

    flags: Qt.Dialog | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
    width: 300
    height: 96
    visible: false
    title: qsTr("fsync")
    color: "#2b2b2b"

    function begin(a) {
        if (win.busy) {
            win.pendingAction = a;
            return;
        }
        // A second command while the window is still up (its close timer is running).
        closer.stop();
        win.action = a;
        win.ran = false;
        win.needsClick = false;
        win.result = "";
        win.show();
        win.raise();
        win.requestActivate();
        maxLife.restart();
        // Already focused: no activeChanged will come, so run now.
        if (win.active)
            win.run();
        else
            fallback.restart();
    }

    function finish() {
        win.hide();
        win.action = "";
        maxLife.stop();
    }

    // Only with the keyboard focus: an unfocused read would return stale or empty data.
    function run() {
        // Never from an unfocused surface: Qt would return stale or empty clipboard data.
        if (win.ran || win.action === "" || !win.active)
            return;
        win.ran = true;
        fallback.stop();
        win.needsClick = false;
        const ran = win.action;
        win.busy = true;
        win.result = QuickClip.run(ran);
        win.busy = false;
        win.lastRunAt = Date.now();
        closer.restart();
        if (win.pendingAction !== "") {
            const next = win.pendingAction;
            win.pendingAction = "";
            // The same command again within 1 s is a double press: drop it.
            if (next !== ran || Date.now() - win.lastRunAt > 1000)
                win.begin(next);
        }
    }

    onActiveChanged: if (active) win.run()

    Timer {
        id: fallback
        interval: 1500
        onTriggered: if (!win.ran) win.needsClick = true
    }

    Timer {
        id: closer
        interval: 1000
        onTriggered: win.finish()
    }

    // A window that never got the focus (nobody clicked) must not stay on screen.
    Timer {
        id: maxLife
        interval: 15000
        onTriggered: win.finish()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Label {
            Layout.fillWidth: true
            Layout.fillHeight: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            color: "white"
            text: win.ran ? win.result
                          : (win.action === "paste" ? qsTr("Pasting...") : qsTr("Sending..."))
        }
        Button {
            Layout.alignment: Qt.AlignHCenter
            visible: win.needsClick && !win.ran
            text: win.action === "paste" ? qsTr("Click to paste") : qsTr("Click to send")
            // The click is user input: ask for the focus and act once the window is active.
            onClicked: {
                if (win.active)
                    win.run();
                else
                    win.requestActivate();
            }
        }
    }
}
