import XCTest

/// Hermetic tests of the CLI process harness (run in a plain `swift test`, no backend needed).
final class CliProcessTests: XCTestCase {
    private func sh(_ script: String) throws -> CliProcess {
        try CliProcess(executable: URL(fileURLWithPath: "/bin/sh"), arguments: ["-c", script])
    }

    func testParsesEventsAndReportsExitAfterDrain() async throws {
        let p = try sh(#"printf '{"event":"a","n":1}\n{"event":"b"}\n'; exit 3"#)
        try await p.expectExit(3, timeout: 5)
        XCTAssertEqual(p.lastEvent("a")?.int("n"), 1)
        XCTAssertNotNil(p.lastEvent("b"))
    }

    func testFinalLineWithoutNewlineIsParsedAtEof() async throws {
        let p = try sh(#"printf '{"event":"x","ok":true}'"#)
        try await p.expectExit(0, timeout: 5)
        XCTAssertEqual(p.lastEvent("x")?.bool("ok"), true)
    }

    func testGrandchildHoldingPipesDoesNotHideExit() async throws {
        let p = try sh(#"printf '{"event":"x"}'; sleep 4 & exit 0"#)
        let start = Date()
        try await p.expectExit(0, timeout: 3)
        XCTAssertLessThan(Date().timeIntervalSince(start), 2.5, "exit must surface after the drain grace")
        XCTAssertNotNil(p.lastEvent("x"))
        await p.awaitExit()
    }

    func testExpectExitTimesOutWithTimeoutError() async throws {
        let p = try sh("sleep 30")
        do {
            try await p.expectExit(0, timeout: 0.3)
            XCTFail("expected a timeout")
        } catch HarnessError.timeout {
        }
        p.terminate()
        await p.awaitExit()
    }

    func testWaitEventFailsWhenProcessExitsWithoutIt() async throws {
        let p = try sh("exit 2")
        do {
            _ = try await p.waitEvent("never", timeout: 5)
            XCTFail("expected an exit error")
        } catch HarnessError.exit(let code, _) {
            XCTAssertEqual(code, 2)
        }
    }

    func testCliEventTypedAccessors() throws {
        let e = CliEvent(["i": 1, "f": 1.5, "t": true, "s": "text", "big": 1e300])
        XCTAssertEqual(e.int("i"), 1)
        XCTAssertNil(e.int("f"), "fractional is not an Int")
        XCTAssertNil(e.int("big"), "out of range is nil, not a trap")
        XCTAssertNil(e.int("s"))
        XCTAssertEqual(e.bool("t"), true)
        XCTAssertNil(e.bool("i"), "1 is a number, not a bool")
        XCTAssertEqual(e.string("s"), "text")
        XCTAssertNil(e.string("missing"))
    }
}
