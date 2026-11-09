#include "clip_access.h"

#include <QtCore/QByteArray>
#include <QtCore/QEventLoop>
#include <QtCore/QTimer>
#include <QtCore/QMimeData>
#include <QtCore/QString>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>

namespace fsync_clip {
namespace {
QMimeData *g_pending = nullptr; // owned until commit_set hands it to the clipboard

QString to_qstring(rust::Str s) {
  return QString::fromUtf8(s.data(), static_cast<qsizetype>(s.size()));
}
} // namespace

rust::Vec<rust::String> formats() {
  rust::Vec<rust::String> out;
  const QClipboard *cb = QGuiApplication::clipboard();
  const QMimeData *md = cb ? cb->mimeData(QClipboard::Clipboard) : nullptr;
  if (md == nullptr) {
    return out;
  }
  for (const QString &f : md->formats()) {
    const QByteArray utf8 = f.toUtf8();
    out.push_back(rust::String(utf8.constData(), static_cast<std::size_t>(utf8.size())));
  }
  return out;
}

rust::Vec<std::uint8_t> data(rust::Str mime) {
  rust::Vec<std::uint8_t> out;
  const QClipboard *cb = QGuiApplication::clipboard();
  const QMimeData *md = cb ? cb->mimeData(QClipboard::Clipboard) : nullptr;
  if (md == nullptr) {
    return out;
  }
  const QByteArray bytes = md->data(to_qstring(mime));
  out.reserve(static_cast<std::size_t>(bytes.size()));
  for (const char c : bytes) {
    out.push_back(static_cast<std::uint8_t>(c));
  }
  return out;
}

void begin_set() {
  delete g_pending;
  g_pending = new QMimeData();
}

void add(rust::Str mime, rust::Slice<const std::uint8_t> bytes) {
  if (g_pending == nullptr) {
    return;
  }
  g_pending->setData(to_qstring(mime),
                     QByteArray(reinterpret_cast<const char *>(bytes.data()),
                                static_cast<qsizetype>(bytes.size())));
}

bool commit_set() {
  QClipboard *cb = QGuiApplication::clipboard();
  if (g_pending == nullptr || cb == nullptr) {
    delete g_pending;
    g_pending = nullptr;
    return false;
  }
  cb->setMimeData(g_pending, QClipboard::Clipboard); // the clipboard owns it from here
  g_pending = nullptr;
  return true;
}

void settle(int ms) {
  QEventLoop loop;
  QTimer::singleShot(ms, &loop, &QEventLoop::quit);
  loop.exec();
}
} // namespace fsync_clip
