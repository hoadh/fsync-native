import FsyncMacCore
import Foundation
import UserNotifications

/// What a notification carries. Text comes only from `IncomingFilesNotice.title/body`; `userInfo` holds the
/// constant `actionKey`/`actionValue` pair and nothing peer-supplied, so Notification Center persists no file names.
public struct IncomingFilesNotification: Equatable, Sendable {
    public static let actionKey = "fsync.action"
    public static let actionValue = "open-files"

    /// Stable per sender, so a later notice from the same sender replaces the earlier one instead of stacking.
    /// This is intended: the coalescer's updated count ("k offers from X") must replace the first banner.
    public var requestID: String
    /// Groups one sender's notices in Notification Center.
    public var threadID: String
    public var title: String
    public var body: String

    /// Text strictly from the notice's sanitized title/body; the sender (already sanitized) only keys the ids.
    public init(notice: IncomingFilesNotice) {
        requestID = "fsync.incoming.\(notice.fromName)"
        threadID = requestID
        title = notice.title
        body = notice.body
    }

    /// Only the constant action key; never names or paths.
    public static let userInfo: [String: String] = [actionKey: actionValue]

    /// A default click on a notification carrying our constant key. Anything else is ignored.
    public static func isOpenFilesClick(actionIdentifier: String, userInfo: [AnyHashable: Any]) -> Bool {
        actionIdentifier == UNNotificationDefaultActionIdentifier
            && userInfo[actionKey] as? String == actionValue
    }

    /// No sound and no badge: arrival is deliberately quiet.
    public func makeContent() -> UNMutableNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.threadIdentifier = threadID
        content.userInfo = Self.userInfo
        content.sound = nil
        return content
    }
}

/// The slice of `UNUserNotificationCenter` the notifier uses; tests substitute a fake.
@MainActor
public protocol NotificationPosting: AnyObject {
    /// Makes `delegate` receive presentation and click callbacks.
    func install(delegate: UNUserNotificationCenterDelegate)
    /// Asks for `.alert` only (no sound, no badge). True when alerts are allowed.
    func requestAuthorization() async -> Bool
    func post(_ notification: IncomingFilesNotification) async throws
    /// Drops the already delivered notification with this request id (after its click).
    func removeDelivered(requestID: String)
}

/// Real center. Only create it inside an `.app` bundle: `UNUserNotificationCenter.current()` crashes without one.
@MainActor
public final class UserNotificationCenterPoster: NotificationPosting {
    private let center: UNUserNotificationCenter

    public init() { center = UNUserNotificationCenter.current() }

    public func install(delegate: UNUserNotificationCenterDelegate) { center.delegate = delegate }

    /// Reads the current setting first, so a user who enabled alerts in System Settings later is picked up.
    public func requestAuthorization() async -> Bool {
        switch await center.notificationSettings().authorizationStatus {
        case .authorized, .provisional: return true
        case .denied: return false
        default: return (try? await center.requestAuthorization(options: [.alert])) ?? false
        }
    }

    public func post(_ notification: IncomingFilesNotification) async throws {
        try await center.add(
            UNNotificationRequest(identifier: notification.requestID, content: notification.makeContent(), trigger: nil))
    }

    public func removeDelivered(requestID: String) {
        center.removeDeliveredNotifications(withIdentifiers: [requestID])
    }
}
