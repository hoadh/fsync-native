#include "app_identity.h"

#include <QtCore/QString>
#include <QtCore/QtGlobal>
#include <QtGui/QGuiApplication>
#include <cstdio>

namespace fsync_identity {
void set_desktop_file_name(rust::Str name) {
  QGuiApplication::setDesktopFileName(
      QString::fromUtf8(name.data(), static_cast<qsizetype>(name.size())));
}

namespace {
// Qt reads XDG_ACTIVATION_TOKEN in requestActivate() from 6.3 on. Older runtimes ignore it, so
// do not touch the environment there: setenv races getenv on other threads (glibc).
bool runtime_reads_activation_token() {
  int major = 0;
  int minor = 0;
  if (std::sscanf(qVersion(), "%d.%d", &major, &minor) != 2) {
    return false;
  }
  return major > 6 || (major == 6 && minor >= 3);
}
} // namespace

bool set_activation_token(rust::Str token) {
  if (!runtime_reads_activation_token()) {
    return false;
  }
  const QByteArray value(token.data(), static_cast<qsizetype>(token.size()));
  return qputenv("XDG_ACTIVATION_TOKEN", value);
}

void clear_activation_token() { qunsetenv("XDG_ACTIVATION_TOKEN"); }
} // namespace fsync_identity
