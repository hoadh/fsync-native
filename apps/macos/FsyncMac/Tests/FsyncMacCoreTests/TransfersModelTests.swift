import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class TransfersModelTests: XCTestCase {
    static func event(_ id: UInt64, _ state: TransferState, done: UInt64 = 0, total: UInt64 = 100,
                      path: String? = nil, message: String? = nil) -> TransferEvent {
        TransferEvent(transferId: id, itemId: "aa", fileId: 0, direction: .receive, state: state, bytesDone: done,
                      bytesTotal: total, path: path, message: message)
    }

    func testProgressUpdatesOneRowAndOutcomeFollowsTheEnd() {
        let model = TransfersModel()
        model.handle(Self.event(1, .started))
        model.handle(Self.event(1, .progress, done: 40))
        XCTAssertEqual(model.rows.count, 1)
        XCTAssertEqual(model.rows[0].fraction, 0.4, accuracy: 0.001)
        XCTAssertEqual(model.active.count, 1)
        XCTAssertNil(model.outcome(of: 1))
        model.handle(Self.event(1, .done, done: 100, path: "/tmp/x"))
        XCTAssertEqual(model.outcome(of: 1), .done(path: "/tmp/x"))
        XCTAssertTrue(model.active.isEmpty)
    }

    func testFailedAndCancelledOutcomes() {
        let model = TransfersModel()
        model.handle(Self.event(1, .failed, message: "boom"))
        model.handle(Self.event(2, .cancelled))
        XCTAssertEqual(model.outcome(of: 1), .failed("boom"))
        XCTAssertEqual(model.outcome(of: 2), .cancelled)
        XCTAssertNil(model.outcome(of: 99))
    }

    func testNameSetBeforeOrAfterFirstEvent() {
        let model = TransfersModel()
        model.setName("a.txt", for: 1)
        model.handle(Self.event(1, .started))
        model.handle(Self.event(2, .started))
        model.setName("b.txt", for: 2)
        XCTAssertEqual(model.rows.first { $0.id == 1 }?.name, "a.txt")
        XCTAssertEqual(model.rows.first { $0.id == 2 }?.name, "b.txt")
    }

    func testWaitOutcomeReturnsWhenTheEndArrivesAndTimesOut() async {
        let model = TransfersModel()
        model.handle(Self.event(1, .started))
        Task { try? await Task.sleep(nanoseconds: 100_000_000); model.handle(Self.event(1, .done, path: "/p")) }
        let ended = await model.waitOutcome(of: 1, timeout: 5)
        XCTAssertEqual(ended, .done(path: "/p"))
        let timedOut = await model.waitOutcome(of: 7, timeout: 0.1)
        XCTAssertEqual(timedOut, .failed("timed out"))
    }

    func testClearFinishedKeepsActiveAndRowsAreBounded() {
        let model = TransfersModel()
        model.handle(Self.event(1, .progress))
        model.handle(Self.event(2, .done))
        model.clearFinished()
        XCTAssertEqual(model.rows.map(\.id), [1])
        for id in 10..<(10 + UInt64(TransfersModel.maxRows) + 20) { model.handle(Self.event(id, .done)) }
        XCTAssertLessThanOrEqual(model.rows.count, TransfersModel.maxRows)
        XCTAssertTrue(model.rows.contains { $0.id == 1 })
        model.reset()
        XCTAssertTrue(model.rows.isEmpty)
    }

    func testWaiterGetsItsOutcomeAfterTheRowIsEvicted() async {
        let model = TransfersModel()
        model.handle(Self.event(1, .done, path: "/x"))
        for id in 2...UInt64(TransfersModel.maxRows + 2) { model.handle(Self.event(id, .started)) }
        XCTAssertFalse(model.rows.contains { $0.id == 1 })
        let outcome = await model.waitOutcome(of: 1, timeout: 1)
        XCTAssertEqual(outcome, .done(path: "/x"))
    }

    func testResetResolvesRunningTransfersAsCancelled() async {
        let model = TransfersModel()
        model.handle(Self.event(7, .progress, done: 1))
        async let waiting = model.waitOutcome(of: 7, timeout: 5)
        try? await Task.sleep(nanoseconds: 100_000_000)
        model.reset()
        let outcome = await waiting
        XCTAssertEqual(outcome, .cancelled)
    }

    func testClearFinishedKeepsOutcomes() async {
        let model = TransfersModel()
        model.handle(Self.event(3, .failed, message: "boom"))
        model.clearFinished()
        XCTAssertTrue(model.rows.isEmpty)
        let outcome = await model.waitOutcome(of: 3, timeout: 1)
        XCTAssertEqual(outcome, .failed("boom"))
    }
}
