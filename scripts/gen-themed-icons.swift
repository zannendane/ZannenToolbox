// 主题图标管线：由暗/亮两张 2048² 源图生成四个 1024² 变体。
//
//   icon-dark.png / icon-light.png         应用图标（运行时可切换）
//   installer-dark.png / installer-light.png  安装包图标（右下角叠加 📦 风格角标）
//
// 用法: swift scripts/gen-themed-icons.swift <暗色源图> <亮色源图> <输出目录>
// 依赖: macOS AppKit（CoreGraphics 合成）。产物入库，Windows CI 无需重跑。

import AppKit

let args = CommandLine.arguments
guard args.count == 4 else {
    print("usage: gen-themed-icons.swift <dark.png> <light.png> <outdir>")
    exit(1)
}
let darkSrc = args[1], lightSrc = args[2], outDir = args[3]
let canvas: CGFloat = 1024

func loadImage(_ path: String) -> NSImage {
    guard let img = NSImage(contentsOfFile: path) else {
        FileHandle.standardError.write("cannot load \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    return img
}

/// 右下角 📦 角标：直接模仿系统 📦 emoji 的纸板箱造型——
/// 棕色箱体（正面 + 顶盖折面）+ 中央胶带 + 左下货运标签，柔和投影。
func drawBadge(_ ctx: CGContext, canvas: CGFloat) {
    let s = canvas * 0.36
    let x = canvas - s * 1.04
    let y = canvas * 0.015 // CG 原点左下 → 视觉右下角

    // 箱体几何
    let bw = s * 0.92          // 正面宽
    let bh = s * 0.60          // 正面高
    let bx = x + s * 0.02
    let by = y + s * 0.02
    let topH = s * 0.26        // 顶盖折面高度
    let inset = s * 0.09       // 顶盖两侧内收

    let cardboardLight = CGColor(srgbRed: 0.875, green: 0.71, blue: 0.51, alpha: 1)  // #DFB582
    let cardboardMid = CGColor(srgbRed: 0.79, green: 0.60, blue: 0.40, alpha: 1)     // #C99A66
    let cardboardDark = CGColor(srgbRed: 0.66, green: 0.48, blue: 0.31, alpha: 1)    // #A87A4F
    let cardboardTop = CGColor(srgbRed: 0.92, green: 0.78, blue: 0.58, alpha: 1)     // #EBC794
    let tape = CGColor(srgbRed: 0.93, green: 0.92, blue: 0.89, alpha: 1)             // 浅灰胶带

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.02), blur: s * 0.07,
                  color: CGColor(gray: 0, alpha: 0.42))

    // —— 正面（圆角微圆，竖向渐变） ——
    let front = CGRect(x: bx, y: by, width: bw, height: bh)
    let frontPath = CGPath(roundedRect: front, cornerWidth: s * 0.035, cornerHeight: s * 0.035, transform: nil)
    ctx.addPath(frontPath)
    ctx.setFillColor(cardboardMid)
    ctx.fillPath()
    ctx.saveGState()
    ctx.addPath(frontPath)
    ctx.clip()
    let frontGrad = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                               colors: [cardboardLight, cardboardMid, cardboardDark] as CFArray,
                               locations: [0, 0.55, 1])!
    ctx.drawLinearGradient(frontGrad, start: CGPoint(x: bx, y: by + bh),
                           end: CGPoint(x: bx, y: by), options: [])
    ctx.restoreGState()

    // —— 顶盖折面（梯形，左右内收） ——
    let topPath = CGMutablePath()
    topPath.move(to: CGPoint(x: bx, y: by + bh))
    topPath.addLine(to: CGPoint(x: bx + inset, y: by + bh + topH))
    topPath.addLine(to: CGPoint(x: bx + bw - inset, y: by + bh + topH))
    topPath.addLine(to: CGPoint(x: bx + bw, y: by + bh))
    topPath.closeSubpath()
    ctx.addPath(topPath)
    ctx.setFillColor(cardboardTop)
    ctx.fillPath()
    // 顶盖与正面交线（折痕）
    ctx.setStrokeColor(CGColor(gray: 0, alpha: 0.16))
    ctx.setLineWidth(s * 0.008)
    ctx.move(to: CGPoint(x: bx, y: by + bh))
    ctx.addLine(to: CGPoint(x: bx + bw, y: by + bh))
    ctx.strokePath()

    // —— 中央胶带：顶盖一段（梯形内）+ 正面一段 ——
    let tapeW = bw * 0.17
    let tcx = bx + bw / 2
    ctx.setFillColor(tape)
    // 顶盖段（跟随顶盖斜边）
    let topTape = CGMutablePath()
    topTape.move(to: CGPoint(x: tcx - tapeW / 2, y: by + bh))
    topTape.addLine(to: CGPoint(x: tcx - tapeW / 2 + inset * 0.92, y: by + bh + topH))
    topTape.addLine(to: CGPoint(x: tcx + tapeW / 2 + inset * 0.92, y: by + bh + topH))
    topTape.addLine(to: CGPoint(x: tcx + tapeW / 2, y: by + bh))
    topTape.closeSubpath()
    ctx.addPath(topTape)
    ctx.setFillColor(CGColor(srgbRed: 0.88, green: 0.87, blue: 0.84, alpha: 1))
    ctx.fillPath()
    // 正面段（裁进正面圆角）
    ctx.saveGState()
    ctx.addPath(frontPath)
    ctx.clip()
    ctx.setFillColor(tape)
    ctx.fill(CGRect(x: tcx - tapeW / 2, y: by, width: tapeW, height: bh))
    // 胶带中缝细线（质感）
    ctx.setStrokeColor(CGColor(gray: 0, alpha: 0.12))
    ctx.setLineWidth(s * 0.006)
    ctx.move(to: CGPoint(x: tcx, y: by))
    ctx.addLine(to: CGPoint(x: tcx, y: by + bh))
    ctx.strokePath()
    ctx.restoreGState()

    // —— 货运标签（正面左下）：白底圆角小片 + 两条灰线 ——
    ctx.saveGState()
    ctx.addPath(frontPath)
    ctx.clip()
    let lw = bw * 0.30, lh = bh * 0.30
    let label = CGRect(x: bx + bw * 0.08, y: by + bh * 0.12, width: lw, height: lh)
    ctx.setFillColor(CGColor(gray: 1, alpha: 0.95))
    ctx.addPath(CGPath(roundedRect: label, cornerWidth: s * 0.02, cornerHeight: s * 0.02, transform: nil))
    ctx.fillPath()
    ctx.setStrokeColor(CGColor(gray: 0.25, alpha: 0.55))
    ctx.setLineWidth(s * 0.012)
    for frac in [0.68, 0.4] as [CGFloat] {
        let ly = label.minY + label.height * frac
        ctx.move(to: CGPoint(x: label.minX + lw * 0.14, y: ly))
        ctx.addLine(to: CGPoint(x: label.maxX - lw * 0.14, y: ly))
        ctx.strokePath()
    }
    ctx.restoreGState()

    ctx.restoreGState()
}

func render(_ src: NSImage, badge: Bool, pixels: Int) -> NSImage {
    let img = NSImage(size: NSSize(width: pixels, height: pixels))
    // 固定像素尺寸：直接用 bitmap rep 作上下文，避免 lockFocus 跟随 Retina 放大
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    ) else { exit(1) }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    src.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels))
    if badge {
        drawBadge(NSGraphicsContext.current!.cgContext, canvas: CGFloat(pixels))
    }
    NSGraphicsContext.restoreGraphicsState()
    img.addRepresentation(rep)
    return img
}

func writePNG(_ img: NSImage, _ path: String, pixels: Int) {
    guard let rep = img.representations.first as? NSBitmapImageRep,
          let data = rep.representation(using: .png, properties: [:]) else {
        FileHandle.standardError.write("encode failed: \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    try! data.write(to: URL(fileURLWithPath: path))
    print("written: \(path) (\(pixels)x\(pixels), \(data.count / 1024)KB)")
}

try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

let dark = loadImage(darkSrc)
let light = loadImage(lightSrc)
// 1024：bundle/安装包母版；512：运行时切换图标（内嵌二进制）
writePNG(render(dark, badge: false, pixels: 1024), "\(outDir)/icon-dark.png", pixels: 1024)
writePNG(render(light, badge: false, pixels: 1024), "\(outDir)/icon-light.png", pixels: 1024)
writePNG(render(dark, badge: false, pixels: 512), "\(outDir)/icon-dark-512.png", pixels: 512)
writePNG(render(light, badge: false, pixels: 512), "\(outDir)/icon-light-512.png", pixels: 512)
writePNG(render(dark, badge: true, pixels: 1024), "\(outDir)/installer-dark.png", pixels: 1024)
writePNG(render(light, badge: true, pixels: 1024), "\(outDir)/installer-light.png", pixels: 1024)
