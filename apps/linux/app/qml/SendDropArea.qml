import QtQuick
import app.fsync

// Local files dropped here are handed to the engine: onto the device `nodeId`, or, with an empty
// `nodeId`, to the DevicePicker. Only `file://` URLs are forwarded (the engine re-checks them).
DropArea {
    id: area
    property string nodeId: ""
    keys: ["text/uri-list"]
    onDropped: function (drop) {
        if (!drop.hasUrls)
            return;
        // The URLs go over as a list: the engine takes their encoded form (names with spaces,
        // `#` or `%` stay exact).
        var urls = drop.urls;
        drop.accept(Qt.CopyAction);
        if (area.nodeId === "")
            SendObject.dropAnywhere(urls);
        else
            SendObject.dropOnDevice(area.nodeId, urls);
    }
}
