import AppKit
import ImageIO
import UniformTypeIdentifiers

let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: dir) }
let source = dir.appendingPathComponent("source.png")
let pixels = (0..<(800 * 400)).flatMap { index -> [UInt8] in
  let x = index % 800
  let y = index / 800
  return [UInt8((x * 73 + y * 17) % 255), UInt8((x * 13 + y * 97) % 255),
          UInt8((x * 31 + y * 7) % 255), 255]
}
let pixelData = Data(pixels) as CFData
let provider = CGDataProvider(data: pixelData)!
let cgImage = CGImage(width: 800, height: 400, bitsPerComponent: 8, bitsPerPixel: 32,
  bytesPerRow: 3200, space: CGColorSpaceCreateDeviceRGB(),
  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
let destination = CGImageDestinationCreateWithURL(source as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(destination, cgImage, nil)
assert(CGImageDestinationFinalize(destination))
let original = try Data(contentsOf: source)
func process(_ options: [String: Any]) throws -> (URL, NSBitmapImageRep) {
  let path = try PickerImageProcessing.process(options.merging(["path": source.path]) { _, new in new })
  let url = URL(fileURLWithPath: path)
  let image = NSBitmapImageRep(data: try Data(contentsOf: url))!
  return (url, image)
}
let (resized, image) = try process(["maxWidth": 200, "maxHeight": 80])
assert(image.pixelsWide == 160 && image.pixelsHigh == 80)
assert(resized.pathExtension == "png")
try FileManager.default.removeItem(at: resized)
let (large, largeImage) = try process(["maxWidth": 1600])
assert(largeImage.pixelsWide == 800 && largeImage.pixelsHigh == 400)
try FileManager.default.removeItem(at: large)
let (low, _) = try process(["imageQuality": 10])
let (high, _) = try process(["imageQuality": 95])
assert(low.pathExtension == "jpg")
let lowSize = try Data(contentsOf: low).count
let highSize = try Data(contentsOf: high).count
assert(lowSize < highSize)
try FileManager.default.removeItem(at: low)
try FileManager.default.removeItem(at: high)
let remaining = try Data(contentsOf: source)
assert(remaining == original)
for options: [String: Any] in [["maxWidth": 0], ["imageQuality": 101], ["maxHeight": Double.infinity]] {
  do { _ = try process(options); fatalError("Accepted invalid options") } catch {}
}
let movie = dir.appendingPathComponent("clip.mov")
try Data([1, 2, 3]).write(to: movie)
let movieResult = try PickerImageProcessing.process(["path": movie.path, "maxWidth": 10, "allowVideo": true])
assert(movieResult == movie.path)
print("PASS: dimensions, aspect ratio, no upscaling, quality, original preservation, validation and video passthrough")
