// QClipboard access for the manual (GNOME) path, GUI thread only. QClipboard is not thread-safe
// and on Wayland only answers while a window of ours has the keyboard focus, which is why the
// QuickClipWindow exists.
#pragma once

#include "rust/cxx.h"
#include <cstdint>

namespace fsync_clip {
// MIME formats currently offered by the clipboard owner.
rust::Vec<rust::String> formats();
// Bytes of one format; empty when it is not there. Blocks the GUI thread while the owner
// answers (Qt's own timeout applies).
rust::Vec<std::uint8_t> data(rust::Str mime);
// Builds a new clipboard entry: begin_set, add* (one call per offered name), commit_set.
void begin_set();
void add(rust::Str mime, rust::Slice<const std::uint8_t> bytes);
bool commit_set();
// Runs the Qt event loop for `ms` so the compositor round trip of a clipboard write (and a
// possible cancellation by another client) is processed before the result is read back.
void settle(int ms);
} // namespace fsync_clip
