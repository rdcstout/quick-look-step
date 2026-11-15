import SceneKit
import Cocoa
import Quartz


/// A tiny helper that converts a STEP file into a ready-to-render `SCNScene`,
/// replicating the same camera and lighting configuration we use in the preview
/// extension so thumbnails and previews look identical before any user
/// interaction.
enum SceneBuilder {
    struct Preview {
        let scene: SCNScene
        let geometryWarning: String?
    }

    enum LoadError: LocalizedError {
        case unreadable
        case fileTooLarge
        case invalidGeometry
        case noGeometry
        case geometryLimit

        var errorDescription: String? {
            switch self {
            case .unreadable: "QuickLookStep could not read this STEP file."
            case .fileTooLarge: "This STEP file exceeds the 128 MB preview limit."
            case .invalidGeometry: "This STEP file contains invalid or unsupported geometry."
            case .noGeometry: "This STEP file contains no supported solid or surface geometry."
            case .geometryLimit: "This STEP assembly is too complex to preview."
            }
        }
    }

    /// Builds a SceneKit scene containing the geometry loaded from the STEP
    /// file at `url`.
    /// - Throws: A `LoadError` if the STEP file cannot be rendered.
    static func preview(for url: URL) throws -> Preview {
        var mesh = MeshSlice()
        let status = url.path.withCString { cPath in
            foxtrot_load_step(cPath, &mesh)
        }
        switch status {
        case FoxtrotLoadSuccess: break
        case FoxtrotLoadUnreadable: throw LoadError.unreadable
        case FoxtrotLoadTooLarge: throw LoadError.fileTooLarge
        case FoxtrotLoadNoGeometry: throw LoadError.noGeometry
        case FoxtrotLoadGeometryLimit: throw LoadError.geometryLimit
        default: throw LoadError.invalidGeometry
        }
        defer { foxtrot_free_mesh(mesh) }

        let vertexCount = Int(mesh.vert_count)
        let indexCount = Int(mesh.tri_count) * 3
        guard vertexCount > 0, indexCount > 0, let vertices = mesh.verts, let triangles = mesh.tris else {
            throw LoadError.noGeometry
        }

        let vertexData = Data(
            bytes: vertices,
            count: vertexCount * 3 * MemoryLayout<Float>.size
        )
        let vertexSource = SCNGeometrySource(
            data: vertexData,
            semantic: .vertex,
            vectorCount: vertexCount,
            usesFloatComponents: true,
            componentsPerVector: 3,
            bytesPerComponent: MemoryLayout<Float>.size,
            dataOffset: 0,
            dataStride: 3 * MemoryLayout<Float>.size
        )

        var sources = [vertexSource]
        if let normals = mesh.normals {
            let components = UnsafeBufferPointer(start: normals, count: vertexCount * 3)
            if components.allSatisfy(\.isFinite) {
                sources.append(SCNGeometrySource(
                    data: Data(bytes: normals, count: vertexData.count),
                    semantic: .normal,
                    vectorCount: vertexCount,
                    usesFloatComponents: true,
                    componentsPerVector: 3,
                    bytesPerComponent: MemoryLayout<Float>.size,
                    dataOffset: 0,
                    dataStride: 3 * MemoryLayout<Float>.size
                ))
            }
        }

        let indexData = Data(
            bytes: triangles,
            count: indexCount * MemoryLayout<UInt32>.size
        )
        let geometryElement = SCNGeometryElement(
            data: indexData,
            primitiveType: .triangles,
            primitiveCount: Int(mesh.tri_count),
            bytesPerIndex: MemoryLayout<UInt32>.size
        )

        let geometry = SCNGeometry(sources: sources, elements: [geometryElement])
        geometry.firstMaterial = SCNMaterial()
        geometry.firstMaterial?.lightingModel = .blinn
        geometry.firstMaterial?.diffuse.contents = NSColor.white
        geometry.firstMaterial?.isDoubleSided = true

        // Assemble the scene graph.
        let scene = SCNScene()
        let node = SCNNode(geometry: geometry)
        scene.rootNode.addChildNode(node)

        // --- Scale & centre geometry ---
        let (minBounds, maxBounds) = node.boundingBox
        let size = SCNVector3(
            maxBounds.x - minBounds.x,
            maxBounds.y - minBounds.y,
            maxBounds.z - minBounds.z
        )

        // Uniformly scale so the largest dimension maps to `targetSize` units.
        let targetSize: CGFloat = 100.0
        let maxExtent = max(size.x, max(size.y, size.z))
        guard maxExtent.isFinite, maxExtent > 0 else {
            throw LoadError.invalidGeometry
        }
        let scaleFactor = targetSize / maxExtent
        let sf = Float(scaleFactor)
        node.scale = SCNVector3(sf, sf, sf)

        // Re-centre so model origin is at (0,0,0)
        let center = SCNVector3(
            (maxBounds.x + minBounds.x) / 2.0,
            (maxBounds.y + minBounds.y) / 2.0,
            (maxBounds.z + minBounds.z) / 2.0
        )
        node.position = SCNVector3(Float(-center.x * scaleFactor),
                                   Float(-center.y * scaleFactor),
                                   Float(-center.z * scaleFactor))

        // --- Camera setup ---
        let cameraNode = SCNNode()
        cameraNode.name = "camera"
        let camera = SCNCamera()
        camera.zNear = 1
        camera.zFar = 10000
        camera.fieldOfView = 45
        cameraNode.camera = camera

        // Place camera so the entire bounding sphere fits inside the view.
        let sx = Float(size.x), sy = Float(size.y), sz = Float(size.z)
        let radius = (sqrt(sx*sx + sy*sy + sz*sz) / 2.0) * sf
        let fovRadians = (Float(camera.fieldOfView) / 2.0) * (.pi / 180.0)
        let distance = radius / tanf(fovRadians)

        // Use a diagonal viewing direction for depth.
        cameraNode.position = SCNVector3(distance, distance, distance)
        cameraNode.look(at: SCNVector3Zero)
        scene.rootNode.addChildNode(cameraNode)

        // --- Lighting ---
        func makeOmni(intensity: CGFloat, position: SCNVector3) -> SCNNode {
            let lightNode = SCNNode()
            lightNode.light = SCNLight()
            lightNode.light?.type = .omni
            lightNode.light?.intensity = intensity
            lightNode.position = position
            return lightNode
        }
        scene.rootNode.addChildNode(makeOmni(intensity: 850, position: cameraNode.position))
        scene.rootNode.addChildNode(makeOmni(intensity: 700, position: SCNVector3(0, 0, -targetSize * 2)))

        let ambientNode = SCNNode()
        ambientNode.light = SCNLight()
        ambientNode.light?.type = .ambient
        ambientNode.light?.color = NSColor(white: 1, alpha: 1)
        ambientNode.light?.intensity = 300
        scene.rootNode.addChildNode(ambientNode)

        let warning = mesh.omitted_faces > 0 ? "Some surfaces could not be displayed" : nil
        return Preview(scene: scene, geometryWarning: warning)
    }
} 
