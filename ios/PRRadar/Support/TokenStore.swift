// GitHub-Token im Schlüsselbund. Beim Start aus Xcode gilt zusätzlich `GITHUB_TOKEN`/`GH_TOKEN`
// aus dem Scheme – praktisch für die Entwicklung, im Store-Build nie gesetzt.

import Foundation
import Security
import Synchronization

enum TokenSource: Equatable {
    case keychain
    case environment

    var label: String {
        switch self {
        case .keychain: "Schlüsselbund"
        case .environment: "GITHUB_TOKEN (Xcode-Scheme)"
        }
    }
}

final class TokenStore: Sendable {
    static let shared = TokenStore()

    private let service = "email.rubeen.prradar"
    private let account = "github-token"
    /// `nil`: noch nicht gelesen; `.some(nil)`: kein Token im Schlüsselbund
    private let cache = Mutex<String??>(nil)

    /// Wird bei jedem GitHub-Request gelesen – ein neuer Token greift sofort.
    var token: String? {
        stored ?? environmentToken
    }

    var source: TokenSource? {
        if stored != nil { return .keychain }
        if environmentToken != nil { return .environment }
        return nil
    }

    private var stored: String? {
        cache.withLock { cached in
            if let cached { return cached }
            let value = read()
            cached = .some(value)
            return value
        }
    }

    private var environmentToken: String? {
        let env = ProcessInfo.processInfo.environment
        return [env["GITHUB_TOKEN"], env["GH_TOKEN"]]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }

    func save(_ token: String) throws {
        let value = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return }
        let query = baseQuery()
        SecItemDelete(query as CFDictionary)
        var add = query
        add[kSecValueData as String] = Data(value.utf8)
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError(status: status) }
        cache.withLock { $0 = .some(value) }
    }

    func delete() {
        SecItemDelete(baseQuery() as CFDictionary)
        cache.withLock { $0 = .some(nil) }
    }

    private func read() -> String? {
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let data = item as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    private func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}

struct KeychainError: LocalizedError {
    var status: OSStatus

    var errorDescription: String? {
        let detail = SecCopyErrorMessageString(status, nil) as String? ?? "Code \(status)"
        return "Token konnte nicht im Schlüsselbund gespeichert werden: \(detail)"
    }
}
