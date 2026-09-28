import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Resizes using oriented pixel dimensions and never overwrites the source.
enum PickerImageProcessing {
  static func process(_ args: [String: Any]) throws -> String {
    guard let path = args["path"] as? String else { throw failure("Missing image path") }
    let url = URL(fileURLWithPath: path)
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else {
      if args["allowVideo"] as? Bool == true,
         let type = UTType(filenameExtension: url.pathExtension), type.conforms(to: .movie) { return path }
      throw failure("Cannot decode image: \(path)")
    }
    if args["allowVideo"] as? Bool == true,
       let type = UTType(filenameExtension: url.pathExtension), type.conforms(to: .movie) {
      return path
    }
    guard let rawProperties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) else {
      throw failure("Missing image dimensions")
    }
    let properties = rawProperties as NSDictionary
    guard let w = properties.object(forKey: "PixelWidth") as? NSNumber,
          let h = properties.object(forKey: "PixelHeight") as? NSNumber else { throw failure("Missing image dimensions") }
    let orientation = (properties.object(forKey: "Orientation") as? NSNumber)?.intValue ?? 1
    let rotated = (5...8).contains(orientation)
    let width = rotated ? h.doubleValue : w.doubleValue
    let height = rotated ? w.doubleValue : h.doubleValue
    let maxWidth = (args["maxWidth"] as? NSNumber)?.doubleValue ?? width
    let maxHeight = (args["maxHeight"] as? NSNumber)?.doubleValue ?? height
    guard maxWidth.isFinite, maxHeight.isFinite, maxWidth > 0, maxHeight > 0 else {
      throw failure("Image dimensions must be finite and positive")
    }
    let quality = (args["imageQuality"] as? NSNumber)?.doubleValue
    guard quality == nil || (quality!.isFinite && (0...100).contains(quality!)) else {
      throw failure("Invalid image quality")
    }
    let scale = min(1, maxWidth / width, maxHeight / height)
    let maxPixels = max(1, floor(max(width, height) * scale))
    guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
      kCGImageSourceCreateThumbnailFromImageAlways: true,
      kCGImageSourceCreateThumbnailWithTransform: true,
      kCGImageSourceThumbnailMaxPixelSize: maxPixels,
    ] as CFDictionary) else { throw failure("Image resize failed") }
    // PNG preserves transparency; an explicit quality requests JPEG compression.
    let png = quality == nil && CGImageSourceGetType(source) == UTType.png.identifier as CFString
    let output = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + (png ? ".png" : ".jpg"))
    guard let destination = CGImageDestinationCreateWithURL(output as CFURL,
      (png ? UTType.png.identifier : UTType.jpeg.identifier) as CFString, 1, nil) else { throw failure("Cannot create image output") }
    CGImageDestinationAddImage(destination, image, [
      kCGImageDestinationLossyCompressionQuality: (quality ?? 100) / 100,
    ] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else {
      try? FileManager.default.removeItem(at: output)
      throw failure("Cannot save processed image")
    }
    return output.path
  }

  private static func failure(_ message: String) -> NSError {
    NSError(domain: "ImagePickerMacOS", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
  }
}
