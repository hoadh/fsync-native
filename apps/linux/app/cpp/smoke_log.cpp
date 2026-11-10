#include "smoke_log.h"

#include <QtCore/QString>
#include <QtCore/QtGlobal>
#include <atomic>
#include <cstdio>
#include <cstring>

namespace {
std::atomic<int> g_qml_warnings{0};
QtMessageHandler g_previous = nullptr;

// A warning counts as a QML problem when it comes from a .qml file or a qml logging category.
bool is_qml_message(const QMessageLogContext &ctx, const QString &msg) {
  if (ctx.file != nullptr && std::strstr(ctx.file, ".qml") != nullptr) {
    return true;
  }
  if (ctx.category != nullptr && std::strncmp(ctx.category, "qt.qml", 6) == 0) {
    return true;
  }
  return msg.contains(QStringLiteral(".qml:"));
}

void handler(QtMsgType type, const QMessageLogContext &ctx, const QString &msg) {
  if (type != QtDebugMsg && type != QtInfoMsg && is_qml_message(ctx, msg)) {
    g_qml_warnings.fetch_add(1);
  }
  if (g_previous != nullptr) {
    g_previous(type, ctx, msg);
  } else {
    // No previous handler means Qt's default one was active: keep its output visible.
    const QByteArray line = qFormatLogMessage(type, ctx, msg).toLocal8Bit();
    std::fprintf(stderr, "%s\n", line.constData());
  }
}
} // namespace

namespace fsync_smoke {
void install_handler() { g_previous = qInstallMessageHandler(handler); }
int qml_warning_count() { return g_qml_warnings.load(); }
} // namespace fsync_smoke
