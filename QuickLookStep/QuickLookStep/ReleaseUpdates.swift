import AppKit
import Foundation

@MainActor
enum ReleaseUpdates {
    private struct Release: Decodable {
        struct Asset: Decodable {
            let name: String
        }

        let tagName: String
        let draft: Bool
        let prerelease: Bool
        let assets: [Asset]
    }

    private static let repository = "https://github.com/rdcstout/quick-look-step"
    private static let assetName = "QuickLookStep-macOS-arm64.zip"
    private static let endpoint = URL(string: "https://api.github.com/repos/rdcstout/quick-look-step/releases/latest")!
    private static let lastAttemptKey = "updateCheckLastAttempt"
    private static let dismissedVersionKey = "updateCheckDismissedVersion"
    private static let interval: TimeInterval = 7 * 24 * 60 * 60
    private static var checking = false

    static func check(manually: Bool) async {
        guard !checking else { return }
        let defaults = UserDefaults.standard
        let now = Date()
        if !manually {
            if let last = defaults.object(forKey: lastAttemptKey) as? Date,
               last <= now, now.timeIntervalSince(last) < interval {
                return
            }
        }
        checking = true
        defaults.set(now, forKey: lastAttemptKey)
        defer { checking = false }

        do {
            var request = URLRequest(url: endpoint, timeoutInterval: 15)
            request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
            let (data, response) = try await URLSession.shared.data(for: request)
            guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                throw URLError(.badServerResponse)
            }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let release = try decoder.decode(Release.self, from: data)
            guard !release.draft, !release.prerelease,
                  release.assets.contains(where: { $0.name == assetName }),
                  let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
                  let currentVersion = versionComponents(current),
                  let availableVersion = versionComponents(release.tagName) else {
                throw URLError(.cannotParseResponse)
            }

            guard currentVersion.lexicographicallyPrecedes(availableVersion) else {
                if manually { showMessage("Quick Look STEP is up to date") }
                return
            }
            guard manually || defaults.string(forKey: dismissedVersionKey) != release.tagName else { return }

            let alert = NSAlert()
            alert.messageText = "A Quick Look STEP update is available"
            alert.informativeText = "Version \(availableVersion.map(String.init).joined(separator: ".")) is ready to download."
            alert.addButton(withTitle: "Download for macOS")
            alert.addButton(withTitle: "Later")
            defaults.set(release.tagName, forKey: dismissedVersionKey)
            if alert.runModal() == .alertFirstButtonReturn,
               let download = URL(string: "\(repository)/releases/latest/download/\(assetName)") {
                NSWorkspace.shared.open(download)
            }
        } catch {
            if manually {
                showMessage("Couldn’t check for updates", detail: "Try again later or visit the Quick Look STEP releases page.")
            }
        }
    }

    private static func versionComponents(_ value: String) -> [Int]? {
        let normalized = value.hasPrefix("release/") ? String(value.dropFirst("release/".count)) : value
        let number = normalized.hasPrefix("v") ? normalized.dropFirst() : normalized[...]
        let components = number.split(separator: ".", omittingEmptySubsequences: false)
        guard (2...3).contains(components.count) else { return nil }
        let values = components.compactMap { component -> Int? in
            guard !component.isEmpty, component.utf8.allSatisfy({ (48...57).contains($0) }) else { return nil }
            return Int(component)
        }
        guard values.count == components.count else { return nil }
        return values.count == 2 ? values + [0] : values
    }

    private static func showMessage(_ title: String, detail: String = "") {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.runModal()
    }
}
