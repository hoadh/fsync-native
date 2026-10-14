import FsyncCoreKit
import XCTest
@testable import FsyncMacCore

@MainActor
final class LoginModelTests: EngineTestCase {
    private func makeModel(signedIn: @escaping @MainActor () -> Void = {}) throws -> (LoginModel, AppSecrets) {
        let secrets = AppSecrets(store: makeStore())
        let engine = try makeEngine(try tempDir(), secrets: secrets)
        return (LoginModel(engine: engine, secrets: secrets, onSignedIn: signedIn), secrets)
    }

    func testEmailValidation() {
        for ok in ["a@b.co", "  a@b.co  ", String(repeating: "a", count: 240) + "@b.co"] {
            XCTAssertTrue(LoginModel.isPlausibleEmail(ok), ok)
        }
        for bad in ["", "  ", "no-at", "@b.co", "a@", String(repeating: "a", count: 260) + "@b.co"] {
            XCTAssertFalse(LoginModel.isPlausibleEmail(bad), bad)
        }
    }

    func testCodeSanitizing() throws {
        XCTAssertEqual(LoginModel.sanitizeCode("12a3-4 56789"), "123456")
        XCTAssertEqual(LoginModel.sanitizeCode("٣٤٥"), "", "only ASCII digits")
        let (model, _) = try makeModel()
        model.code = "1x2y3"
        XCTAssertEqual(model.code, "123")
        XCTAssertFalse(model.canVerify)
    }

    func testInvalidEmailMakesNoEngineCall() async throws {
        let (model, _) = try makeModel()
        model.email = "nope"
        XCTAssertFalse(model.canSendCode)
        await model.sendCode()
        XCTAssertEqual(model.step, .email)
        XCTAssertNil(model.errorMessage, "an engine call would have failed with a network error")
    }

    func testSendCodeOfflineShowsNetworkTextAndStaysOnEmail() async throws {
        let (model, _) = try makeModel()
        model.email = "a@b.co"
        await model.sendCode()
        XCTAssertEqual(model.errorMessage, "Can't reach fsync. Check your connection.")
        XCTAssertFalse(model.isBusy)
        XCTAssertEqual(model.step, .email)
    }

    func testVerifyOfflineKeepsNoSessionAndClearsCode() async throws {
        var signedIn = false
        let (model, secrets) = try makeModel { signedIn = true }
        model.verify_precondition()
        model.code = "123456"
        await model.verify()
        XCTAssertEqual(model.errorMessage, "Can't reach fsync. Check your connection.")
        XCTAssertEqual(model.code, "", "the code field is cleared after an error")
        XCTAssertEqual(model.step, .code(email: "a@b.co"), "stays on the code screen")
        XCTAssertFalse(model.isBusy)
        XCTAssertNil(try secrets.session())
        XCTAssertFalse(signedIn)
    }

    func testVerifyFromEmailStepDoesNothing() async throws {
        let (model, _) = try makeModel()
        model.code = "123456"
        await model.verify()
        XCTAssertNil(model.errorMessage)
    }
}

@MainActor
private extension LoginModel {
    /// Puts the model on the code step without a backend (sending needs a server).
    func verify_precondition() { step = .code(email: "a@b.co") }
}
