// Rebuild the welcome wordmark from the same bundled font as the native UI.
// Outlines keep the soft text shadow identical in browsers and GPUI's cached
// image renderer without introducing a runtime font or raster asset dependency.
import Foundation
import CoreText
import CoreGraphics

let arguments = CommandLine.arguments
guard arguments.count == 3 else { throw NSError(domain: "altaed.wordmark", code: 1, userInfo: [NSLocalizedDescriptionKey: "Usage: generate_wordmark.swift FONT_PATH OUTPUT_DIRECTORY"]) }
let fontURL = URL(fileURLWithPath: arguments[1])
guard let provider = CGDataProvider(url: fontURL as CFURL), let graphicsFont = CGFont(provider) else { throw NSError(domain: "altaed.wordmark", code: 2, userInfo: [NSLocalizedDescriptionKey: "Cannot load the UI font"]) }
let font = CTFontCreateWithGraphicsFont(graphicsFont, 28, nil, nil)
let characters = Array("altaed".utf16)
var glyphs = [CGGlyph](repeating: 0, count: characters.count)
guard CTFontGetGlyphsForCharacters(font, characters, &glyphs, characters.count) else { throw NSError(domain: "altaed.wordmark", code: 3) }
var advances = [CGSize](repeating: .zero, count: glyphs.count)
CTFontGetAdvancesForGlyphs(font, .horizontal, glyphs, &advances, glyphs.count)
let width = advances.reduce(0) { $0 + $1.width }
let padding: CGFloat = 12
let height: CGFloat = 38
let baseline = (height - CTFontGetAscent(font) - CTFontGetDescent(font)) / 2 + CTFontGetAscent(font)
func number(_ value: CGFloat) -> String { String(format: "%.4f", Double(value)) }
var path = ""
var advance: CGFloat = 0
for (index, glyph) in glyphs.enumerated() {
    if let outline = CTFontCreatePathForGlyph(font, glyph, nil) {
        outline.applyWithBlock { element in
            let value = element.pointee
            func point(_ index: Int) -> String { let p = value.points[index]; return "\(number(p.x + advance + padding)) \(number(baseline - p.y + padding))" }
            switch value.type {
            case .moveToPoint: path += "M\(point(0))"
            case .addLineToPoint: path += "L\(point(0))"
            case .addQuadCurveToPoint: path += "Q\(point(0)) \(point(1))"
            case .addCurveToPoint: path += "C\(point(0)) \(point(1)) \(point(2))"
            case .closeSubpath: path += "Z"
            @unknown default: break
            }
        }
    }
    advance += advances[index].width
}
let svgWidth = width + padding * 2
let svgHeight = height + padding * 2
let svg = """
<svg xmlns="http://www.w3.org/2000/svg" width="\(number(svgWidth*2))" height="\(number(svgHeight*2))" viewBox="0 0 \(number(svgWidth)) \(number(svgHeight))"><defs><path id="word" d="\(path)"/><filter id="shadow" x="-30%" y="-80%" width="160%" height="280%"><feGaussianBlur stdDeviation="3.2"/></filter></defs><use href="#word" transform="translate(0 5)" fill="#315e91" fill-opacity=".16" filter="url(#shadow)"/><use href="#word" fill="#315e91"/></svg>
"""
let output = URL(fileURLWithPath: arguments[2], isDirectory: true)
try svg.write(to: output.appendingPathComponent("welcome-wordmark.svg"), atomically: true, encoding: .utf8)
let metrics = ["width": width, "height": height, "padding": padding]
try JSONSerialization.data(withJSONObject: metrics, options: [.sortedKeys, .prettyPrinted]).write(to: output.appendingPathComponent("welcome-wordmark.json"))
