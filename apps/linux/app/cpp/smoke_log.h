// Message handler for `--smoke`: counts QML warnings so a broken QML file fails the smoke run.
#pragma once

namespace fsync_smoke {
void install_handler();
int qml_warning_count();
} // namespace fsync_smoke
