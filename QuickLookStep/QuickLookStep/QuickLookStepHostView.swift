import SwiftUI
import SceneKit
import Quartz
import Metal
import AppKit
import UniformTypeIdentifiers

/// Root UI used inside the demo host macOS app.  Drop a .step file to view it.
struct QuickLookStepHostView: View {
    private static let loadQueue = DispatchQueue(label: "com.rdcstout.QuickLookStep.file", qos: .userInitiated)

    @State private var scene: SCNScene? = nil
    @State private var loadError: String? = nil
    @State private var geometryWarning: String?
    @State private var isTargeted: Bool = false
    @State private var currentLoad: UUID?

    var body: some View {
        ZStack {
            if let scene {
                SceneKitView(scene: scene)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Text("Drop a STEP file")
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }

            if let message = loadError ?? geometryWarning {
                VStack {
                    Spacer()
                    Text(message)
                        .foregroundStyle(.blue)
                        .padding()
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity) // fill the window
        .contentShape(Rectangle()) // define hit-test area after sizing
        .onDrop(of: [UTType.fileURL, UTType.url, UTType.item], isTargeted: $isTargeted) { providers in
            var collected: [URL] = []
            let group = DispatchGroup()

            func load(_ provider: NSItemProvider, uti: String) {
                group.enter()
                provider.loadItem(forTypeIdentifier: uti, options: nil) { item, _ in
                    let candidate: URL?
                    if let url = item as? URL {
                        candidate = url
                    } else if let data = item as? Data {
                        candidate = URL(dataRepresentation: data, relativeTo: nil)
                    } else if let path = item as? String {
                        candidate = URL(fileURLWithPath: path)
                    } else {
                        candidate = nil
                    }
                    DispatchQueue.main.async {
                        if let candidate, candidate.isFileURL {
                            collected.append(candidate)
                        }
                        group.leave()
                    }
                }
            }

            for provider in providers {
                if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                    load(provider, uti: UTType.fileURL.identifier)
                } else if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
                    load(provider, uti: UTType.url.identifier)
                } else if provider.hasItemConformingToTypeIdentifier(UTType.item.identifier) {
                    load(provider, uti: UTType.item.identifier)
                }
            }

            group.notify(queue: .main) {
                if let first = collected.first {
                    loadStep(first)
                }
            }

            return true
        }
        .background(isTargeted ? Color.accentColor.opacity(0.2) : Color.clear)
        // Respond when the app is launched/opened with a file (Finder double-click, `open` CLI, etc.)
        .onOpenURL { url in
            guard url.isFileURL else { return }
            loadStep(url)
        }
    }

    // MARK: - Loading & snapshot
    private func loadStep(_ url: URL) {
        loadError = nil
        geometryWarning = nil
        let loadID = UUID()
        currentLoad = loadID

        Self.loadQueue.async {
            let needsSecurity = url.startAccessingSecurityScopedResource()
            defer {
                if needsSecurity { url.stopAccessingSecurityScopedResource() }
            }

            let result = Result { try SceneBuilder.preview(for: url) }
            DispatchQueue.main.async {
                guard currentLoad == loadID else { return }
                switch result {
                case .success(let preview):
                    scene = preview.scene
                    geometryWarning = preview.geometryWarning
                case .failure(let error):
                    scene = nil
                    loadError = error.localizedDescription
                }
            }
        }
    }
}

// MARK: - Drag & Drop helper (legacy – unused) 
