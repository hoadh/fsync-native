// Window identity and xdg-activation hand-off for the main window (GUI thread only).
#pragma once

#include "rust/cxx.h"

namespace fsync_identity {
// QGuiApplication::setDesktopFileName: Wayland app_id and the desktop-entry match.
void set_desktop_file_name(rust::Str name);
// XDG_ACTIVATION_TOKEN for the next requestActivate() call.
// False when this Qt (runtime < 6.3) would not read it: nothing is changed then.
bool set_activation_token(rust::Str token);
void clear_activation_token();
} // namespace fsync_identity
