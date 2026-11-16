//
//  ThumbnailProvider.swift
//  StepThumbnail
//
//  Created by John Boiles on 7/14/25.
//

import QuickLookThumbnailing
import SceneKit
import Cocoa
import Metal

class ThumbnailProvider: QLThumbnailProvider {
    private static let renderQueue = DispatchQueue(
        label: "com.rdcstout.QuickLookStep.thumbnail",
        qos: .utility
    )
    private static let availableSlots = DispatchSemaphore(value: 4)
    // Accessed only on renderQueue. Share the Metal device across requests,
    // but release each renderer with its scene and GPU resources.
    private static let device = MTLCreateSystemDefaultDevice()

    override func provideThumbnail(for request: QLFileThumbnailRequest, _ handler: @escaping (QLThumbnailReply?, Error?) -> Void) {
        guard Self.availableSlots.wait(timeout: .now()) == .success else {
            handler(nil, NSError(domain: "ThumbnailProvider", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "STEP thumbnail renderer is busy"
            ]))
            return
        }

        Self.renderQueue.async {
            defer { Self.availableSlots.signal() }
            do {
                let reply = try autoreleasepool { try Self.renderThumbnail(for: request) }
                handler(reply, nil)
            } catch {
                handler(nil, error)
            }
        }
    }

    private static func renderThumbnail(for request: QLFileThumbnailRequest) throws -> QLThumbnailReply {
        let pointSize = request.maximumSize
        let scale = CGFloat(request.scale)
        let longestEdge = max(pointSize.width, pointSize.height)
        guard pointSize.width.isFinite, pointSize.height.isFinite,
              pointSize.width > 0, pointSize.height > 0, scale.isFinite, scale > 0 else {
            throw NSError(domain: "ThumbnailProvider", code: 2, userInfo: [
                NSLocalizedDescriptionKey: "Invalid thumbnail size"
            ])
        }

        let renderScale = min(scale, 1024 / longestEdge)
        let pixelSize = CGSize(
            width: max(1, (pointSize.width * renderScale).rounded()),
            height: max(1, (pointSize.height * renderScale).rounded())
        )
        let scene = try SceneBuilder.preview(for: request.fileURL).scene
        scene.background.contents = NSColor(srgbRed: 0.18, green: 0.19, blue: 0.21, alpha: 1)
        let renderer = SCNRenderer(device: device, options: nil)
        renderer.scene = scene
        renderer.pointOfView = scene.rootNode.childNode(withName: "camera", recursively: true)

        let image = renderer.snapshot(atTime: 0, with: pixelSize, antialiasingMode: .multisampling2X)
        guard let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
            throw NSError(domain: "ThumbnailProvider", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "Failed to create a thumbnail image"
            ])
        }

        return QLThumbnailReply(contextSize: pointSize, drawing: { context in
            // Quick Look can supply a pixel-sized bitmap without a points-to-pixels transform.
            let bitmapBounds = CGRect(x: 0, y: 0, width: context.width, height: context.height)
            context.draw(cgImage, in: context.convertToUserSpace(bitmapBounds))
            return true
        })
    }
}
