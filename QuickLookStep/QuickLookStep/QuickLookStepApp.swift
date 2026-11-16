//
//  QuickLookStepApp.swift
//  QuickLookStep
//
//  Created by John Boiles on 7/14/25.
//

import SwiftUI

@main
struct QuickLookStepApp: App {
    @AppStorage("automaticallyCheckForUpdates") private var automaticallyCheckForUpdates = true

    var body: some Scene {
        WindowGroup {
            QuickLookStepHostView()
                .task(id: automaticallyCheckForUpdates) {
                    guard automaticallyCheckForUpdates else { return }
                    while !Task.isCancelled {
                        await ReleaseUpdates.check(manually: false)
                        do {
                            try await Task.sleep(for: .seconds(3600))
                        } catch {
                            return
                        }
                    }
                }
        }
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") {
                    Task { await ReleaseUpdates.check(manually: true) }
                }
                Toggle("Automatically Check for Updates", isOn: $automaticallyCheckForUpdates)
            }
        }
    }
}
