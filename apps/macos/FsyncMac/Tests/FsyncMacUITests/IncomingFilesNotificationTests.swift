import UserNotifications
import XCTest
@testable import FsyncMacCore
@testable import FsyncMacUI

/// The pure parts of a notification: ids, content, and the click filter.
@MainActor
final class IncomingFilesNotificationTests: XCTestCase {
    private let notice = IncomingFilesNotice(fromName: "Alice", fileCount: 2, totalBytes: 2048, autoAccepted: true)

    func testRequestIDsAreStablePerSender() {
        let a1 = IncomingFilesNotification(notice: notice)
        let a2 = IncomingFilesNotification(
            notice: IncomingFilesNotice(fromName: "Alice", fileCount: 9, totalBytes: 9, autoAccepted: false, offerCount: 3))
        let b = IncomingFilesNotification(
            notice: IncomingFilesNotice(fromName: "Bob", fileCount: 1, totalBytes: 1, autoAccepted: true))
        XCTAssertEqual(a1.requestID, a2.requestID)
        XCTAssertEqual(a1.threadID, a2.threadID)
        XCTAssertNotEqual(a1.requestID, b.requestID)
        XCTAssertNotEqual(a1.threadID, b.threadID)
    }

    func testContentCarriesOnlyConstantKeyAndNoSound() {
        let content = IncomingFilesNotification(notice: notice).makeContent()
        XCTAssertEqual(content.userInfo.count, 1)
        XCTAssertEqual(content.userInfo[IncomingFilesNotification.actionKey] as? String, IncomingFilesNotification.actionValue)
        XCTAssertNil(content.sound)
        XCTAssertEqual(content.title, notice.title)
        XCTAssertEqual(content.body, notice.body)
    }

    func testClickFilter() {
        let good: [AnyHashable: Any] = [IncomingFilesNotification.actionKey: IncomingFilesNotification.actionValue]
        let isClick = IncomingFilesNotification.isOpenFilesClick
        XCTAssertTrue(isClick(UNNotificationDefaultActionIdentifier, good))
        XCTAssertFalse(isClick(UNNotificationDismissActionIdentifier, good))
        XCTAssertFalse(isClick("other", good))
        XCTAssertFalse(isClick(UNNotificationDefaultActionIdentifier, [:]))
        XCTAssertFalse(isClick(UNNotificationDefaultActionIdentifier, [IncomingFilesNotification.actionKey: "evil"]))
    }
}
