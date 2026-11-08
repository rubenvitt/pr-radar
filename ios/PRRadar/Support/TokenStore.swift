// GitHub-Token im Schlüsselbund. Beim Start aus Xcode gilt zusätzlich `GITHUB_TOKEN`/`GH_TOKEN`
// aus dem Scheme – praktisch für die Entwicklung, im Store-Build nie gesetzt.

import Foundation
import Security
import Synchronization

enum TokenSource: Equatable {
    /// Per „Mit GitHub anmelden“ (Device Flow) geholt, liegt im Schlüsselbund
    case login
    /// Von Hand eingetragener Personal Access Token im Schlüsselbund
    case keychain
    case environment

    var label: String {
        switch self {
        case .login: "GitHub-Anmeldung"
        case .keychain: "Personal Access Token"
        case .environment: "GITHUB_TOKEN (Xcode-Scheme)"
        }
    }
}

final class TokenStore: Sendable {
    static let shared = TokenStore()

    private let service: String
    private let account: String
    private let environment: @Sendable () -> [String: String]
    /// `nil`: noch nicht gelesen; `.some(nil)`: kein Token im Schlüsselbund
    private let cache = Mutex<String??>(nil)

    /// Tests geben einen eigenen `service` und eine feste Umgebung vor, damit sie den Token der App nicht berühren.
    init(
        service: String = "email.rubeen.prradar",
        account: String = "github-token",
        environment: @escaping @Sendable () -> [String: String] = { ProcessInfo.processInfo.environment }
    ) {
        self.service = service
        self.account = account
        self.environment = environment
    }

    /// Wird bei jedem GitHub-Request gelesen – ein neuer Token greift sofort.
    var token: String? {
        stored ?? environmentToken
    }

    var source: TokenSource? {
        if stored != nil { return UserDefaults.standard.bool(forKey: fromLoginKey) ? .login : .keychain }
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
        let env = environment()
        return [env["GITHUB_TOKEN"], env["GH_TOKEN"]]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }

    /// Merkt sich, ob der Token im Schlüsselbund aus der Anmeldung stammt (kein Geheimnis).
    private var fromLoginKey: String { "\(service).\(account).fromLogin" }

    /// `fromLogin`: Token aus dem Device Flow statt eines eingetragenen PAT.
    func save(_ token: String, fromLogin: Bool = false) throws {
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
        UserDefaults.standard.set(fromLogin, forKey: fromLoginKey)
    }

    func delete() {
        SecItemDelete(baseQuery() as CFDictionary)
        UserDefaults.standard.removeObject(forKey: fromLoginKey)
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
