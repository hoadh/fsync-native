import AppKit
import FsyncCoreKit
import FsyncMacCore
import SwiftUI

/// This Mac is new: shows a code or QR and waits for the other device.
struct PairOfferView: View {
    @ObservedObject var model: PairingModel

    var body: some View {
        VStack(spacing: 16) {
            if case .offering(let offer, let mode) = model.state {
                Picker("", selection: Binding(get: { mode }, set: { new in Task { await model.offer(new) } })) {
                    Text("Code").tag(PairingMode.code)
                    Text("QR").tag(PairingMode.qr)
                }
                .pickerStyle(.segmented).labelsHidden().frame(width: 180)
                .disabled(model.isWorking)
                if mode == .qr, let payload = offer.qrPayload {
                    QrCodeImageView(text: payload)
                } else if let code = offer.code {
                    CodeBlock(code: code)
                }
                Countdown(expiresMs: offer.expiresMs) { Task { await model.offer(mode) } }
            } else {
                ProgressView()
                Text("Getting a pairing code…").foregroundStyle(.secondary)
            }
        }
        .task {
            // Start once per appearance, and only when nothing is open yet.
            if model.state == .idle, !model.isWorking { await model.offer(.code) }
        }
    }
}

private struct CodeBlock: View {
    let code: String

    var body: some View {
        VStack(spacing: 8) {
            Text(verbatim: PairingCode.grouped(code))
                .font(.system(size: 30, weight: .semibold, design: .monospaced))
                .textSelection(.enabled)
            Button("Copy code") {
                PairingPasteboard.copy(code)
            }
        }
    }
}

private struct Countdown: View {
    let expiresMs: UInt64
    let renew: () -> Void

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let left = PairingCode.secondsLeft(expiresMs: expiresMs, now: context.date)
            if left > 0 {
                Text("Expires in \(left / 60):\(String(format: "%02d", left % 60))").foregroundStyle(.secondary).monospacedDigit()
            } else {
                Button("Get a new code", action: renew)
            }
        }
    }
}
