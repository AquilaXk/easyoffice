import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import SwiftUI

public struct DocumentHorizontalRulerView: View {
  public let document: OfficeDocumentModel
  public let selectedParagraph: ParagraphBlock?
  public let palette: WorkspaceThemePalette
  public var paperMaxWidth: CGFloat
  public var viewModel: WorkspaceViewModel?

  public init(
    document: OfficeDocumentModel,
    selectedParagraph: ParagraphBlock?,
    palette: WorkspaceThemePalette,
    paperMaxWidth: CGFloat = 820.0,
    viewModel: WorkspaceViewModel? = nil
  ) {
    self.document = document
    self.selectedParagraph = selectedParagraph
    self.palette = palette
    self.paperMaxWidth = paperMaxWidth
    self.viewModel = viewModel
  }

  public var body: some View {
    let customPadding = viewModel?.documentHorizontalPadding ?? 28.0
    let leftMarginPt: CGFloat = max(24.0, min(96.0, CGFloat(customPadding + 26.0)))
    let rightMarginPt: CGFloat = leftMarginPt
    let firstLineOffsetPt = CGFloat(selectedParagraph?.firstLineIndentPt ?? 0.0)
    let leftIndentOffsetPt = CGFloat(selectedParagraph?.leftIndentPt ?? 0.0)
    let rulerBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#25292F" : "#FFFFFF",
      fallback: palette.cardSurface.swiftUIColor
    )
    let marginShade = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#1F2329" : "#D4D4D4",
      fallback: palette.elevatedSurface.swiftUIColor
    )

    GeometryReader { geo in
      let totalWidth = max(240.0, geo.size.width)
      let clampedPaperWidth = min(totalWidth, max(320.0, paperMaxWidth))
      let paperOffset = max(0.0, (totalWidth - clampedPaperWidth) * 0.5)
      let effectiveLeft = paperOffset + leftMarginPt
      let effectiveRight = paperOffset + rightMarginPt
      let bodyWidth = max(80.0, totalWidth - effectiveLeft - effectiveRight)
      let cmStep = bodyWidth / 40.0

      ZStack(alignment: .topLeading) {
        Rectangle()
          .fill(rulerBg)

        Rectangle()
          .fill(marginShade)
          .frame(width: effectiveLeft, height: 20)

        Rectangle()
          .fill(marginShade)
          .frame(width: effectiveRight, height: 20)
          .offset(x: totalWidth - effectiveRight)

        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          for unitIndex in 0...80 {
            let xPos = effectiveLeft + (CGFloat(unitIndex) * 0.5) * cmStep
            guard xPos <= totalWidth - effectiveRight + 1.0 else { continue }
            let isMajor = (unitIndex % 2 == 0)
            let tickHeight: CGFloat = isMajor ? 7.0 : 3.5
            let tickWidth: CGFloat = isMajor ? 0.9 : 0.6
            let rect = CGRect(
              x: xPos - tickWidth * 0.5,
              y: size.height - tickHeight,
              width: tickWidth,
              height: tickHeight
            )
            context.fill(
              Path(rect),
              with: .color(tickColor.opacity(isMajor ? 0.8 : 0.45))
            )
          }
        }

        ForEach(1...40, id: \.self) { unit in
          if unit % 2 == 1 || unit == 40 {
            let xPos = effectiveLeft + CGFloat(unit) * cmStep
            if xPos <= totalWidth - effectiveRight + 4.0 {
              Text("\(unit)")
                .font(.system(size: 7.0, weight: .semibold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .position(x: xPos, y: 6)
            }
          }
        }

        Text("▽")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
          .position(
            x: min(
              totalWidth - effectiveRight, effectiveLeft + leftIndentOffsetPt + firstLineOffsetPt),
            y: 4.5
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newFirst = max(
                  0.0,
                  min(144.0, Double(drag.location.x - effectiveLeft - leftIndentOffsetPt))
                )
                viewModel?.setActiveParagraphIndent(firstLineIndentPt: newFirst)
              }
          )
          .onTapGesture {
            let currentFirst = Double(firstLineOffsetPt)
            let nextFirst = currentFirst >= 24.0 ? 0.0 : (currentFirst + 12.0)
            viewModel?.setActiveParagraphIndent(firstLineIndentPt: nextFirst)
          }
          .help("첫 줄 들여쓰기 마커 (\(Int(firstLineOffsetPt))pt)")

        Text("△")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
          .position(
            x: min(totalWidth - effectiveRight, effectiveLeft + leftIndentOffsetPt),
            y: 15.5
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newLeft = max(0.0, min(144.0, Double(drag.location.x - effectiveLeft)))
                viewModel?.setActiveParagraphIndent(leftIndentPt: newLeft)
              }
          )
          .onTapGesture {
            let currentLeft = Double(leftIndentOffsetPt)
            let nextLeft = currentLeft >= 36.0 ? 0.0 : (currentLeft + 18.0)
            viewModel?.setActiveParagraphIndent(leftIndentPt: nextLeft)
          }
          .help("좌측 단락 들여쓰기 마커 (\(Int(leftIndentOffsetPt))pt)")

        Text("▽")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .position(x: totalWidth - effectiveRight, y: 15.5)
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let distFromRight = max(
                  24.0, min(96.0, Double(totalWidth - paperOffset - drag.location.x)))
                viewModel?.setDocumentHorizontalPadding(max(16.0, distFromRight - 26.0))
              }
          )
          .help("오른쪽 여백 경계 (\(Int(rightMarginPt))pt)")
      }
      .border(palette.border.swiftUIColor, width: 1)
    }
    .frame(height: 20)
  }
}

public struct DocumentVerticalRulerView: View {
  public let palette: WorkspaceThemePalette

  public init(palette: WorkspaceThemePalette) {
    self.palette = palette
  }

  public var body: some View {
    let rulerBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#25292F" : "#FFFFFF",
      fallback: palette.cardSurface.swiftUIColor
    )
    let marginShade = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#1F2329" : "#D4D4D4",
      fallback: palette.elevatedSurface.swiftUIColor
    )

    VStack(spacing: 0) {
      ZStack(alignment: .bottomTrailing) {
        Rectangle()
          .fill(marginShade)
          .frame(width: 20, height: 34)

        Text("0")
          .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .frame(width: 14, alignment: .center)
          .offset(x: -4, y: -2)

        Rectangle()
          .fill(palette.hairlineBorder)
          .frame(height: 0.8)
      }

      ForEach(1...36, id: \.self) { cm in
        ZStack(alignment: .topTrailing) {
          Rectangle()
            .fill(palette.secondaryText.swiftUIColor.opacity(0.75))
            .frame(width: 5.5, height: 0.9)

          Text("\(cm)")
            .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .frame(width: 14, alignment: .center)
            .offset(x: -4, y: 1)

          Rectangle()
            .fill(palette.hairlineBorder)
            .frame(width: 3.0, height: 0.7)
            .offset(y: 10)
        }
        .frame(width: 20, height: 20)
      }

      Spacer(minLength: 0)
    }
    .frame(width: 20)
    .frame(maxHeight: .infinity, alignment: .top)
    .background(rulerBg)
    .border(palette.hairlineBorder, width: 1)
  }
}

public struct HWPManuscriptGridPaperView: View {
  public let document: OfficeDocumentModel
  public let palette: WorkspaceThemePalette
  public var viewModel: WorkspaceViewModel?

  public init(
    document: OfficeDocumentModel,
    palette: WorkspaceThemePalette,
    viewModel: WorkspaceViewModel? = nil
  ) {
    self.document = document
    self.palette = palette
    self.viewModel = viewModel
  }

  private var manuscriptPages: [[String]] {
    let rawText = document.paragraphs.map(\.text).joined(separator: "\n")
    var cells: [String] = []
    for ch in rawText {
      if ch == "\n" {
        let remainder = cells.count % 20
        if remainder != 0 {
          let padCount = 20 - remainder
          for _ in 0..<padCount {
            cells.append(" ")
          }
        }
      } else {
        cells.append(String(ch))
      }
    }
    if cells.isEmpty {
      cells = Array(repeating: " ", count: 200)
    }
    var pages: [[String]] = []
    var offset = 0
    while offset < cells.count {
      let end = min(cells.count, offset + 200)
      var slice = Array(cells[offset..<end])
      if slice.count < 200 {
        slice.append(contentsOf: Array(repeating: " ", count: 200 - slice.count))
      }
      pages.append(slice)
      offset += 200
    }
    return pages
  }

  public var body: some View {
    let pages = manuscriptPages
    let gridStrokeColor = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#D97706" : "#C84B31",
      fallback: palette.border.swiftUIColor
    )
    let fullText = document.paragraphs.map(\.text).joined(separator: "\n")

    VStack(alignment: .leading, spacing: 14) {
      if let vm = viewModel, !vm.isCanvasSnapshotMode {
        VStack(alignment: .leading, spacing: 4) {
          Text("원고지 본문 직접 편집 (입력 즉시 200자 원고지 격자에 반영)")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          TextEditor(
            text: Binding(
              get: { fullText },
              set: { vm.updateActiveDocumentManuscriptFullText($0) }
            )
          )
          .font(.system(size: 12, design: .monospaced))
          .frame(minHeight: 68, maxHeight: 110)
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .border(gridStrokeColor.opacity(0.6), width: 1)
        }
      }

      ForEach(Array(pages.enumerated()), id: \.offset) { pageIdx, pageCells in
        VStack(alignment: .leading, spacing: 6) {
          HStack {
            Label(
              "원고지 \(pageIdx + 1)매 / 총 \(pages.count)매 (20×10 = 200자)",
              systemImage: "squareshape.split.3x3"
            )
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(gridStrokeColor)

            Spacer()

            Text(document.title)
              .font(.system(size: 10, weight: .medium))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          }

          VStack(spacing: 3) {
            ForEach(0..<10, id: \.self) { rowIdx in
              HStack(spacing: 0) {
                ForEach(0..<20, id: \.self) { colIdx in
                  let cellIdx = rowIdx * 20 + colIdx
                  let ch = pageCells.indices.contains(cellIdx) ? pageCells[cellIdx] : " "
                  Text(ch)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .frame(maxWidth: .infinity, minHeight: 24, maxHeight: 24)
                    .background(palette.cardSurface.swiftUIColor)
                    .border(gridStrokeColor.opacity(0.65), width: 0.7)
                }
              }
              .border(gridStrokeColor, width: 1.0)
            }
          }
          .padding(8)
          .background(palette.elevatedSurface.swiftUIColor)
          .border(gridStrokeColor, width: 1.6)
        }
      }
    }
  }
}

struct StudioProcessFlowDiagramView: View {
  var body: some View {
    let boxFill = EasyOfficeDesignTokens.resolveHexColor("#F5E6D3", fallback: Color.orange.opacity(0.15))
    let borderColor = EasyOfficeDesignTokens.resolveHexColor("#B8A082", fallback: Color.gray)
    let textColor = EasyOfficeDesignTokens.resolveHexColor("#111827", fallback: .black)
    let steps: [(num: String, title: String, sub: String?)] = [
      ("①", "적합성검토", nil),
      ("②", "선정평가(평가위원회)", "서류평가 , 발표평가"),
      ("③", "사업비 심의·조정", "(사업비검토위원회)"),
      ("④", "중복 심의", "(중복검토위원회)"),
    ]
    let criteriaItems = [
      "○ 공급기업 POOL(붙임5) 미등록 공급기업과 매칭하여 과제 신청하는 경우",
      "○ 지원사업 참여제한중인 수요 및 공급기업이 신청하는 경우",
      "○ 과거 지원받은 수요기업이 동일 과제로 재신청 하는 경우",
      "○ 수요기업과 공급기업의 대표자가 동일인이거나 특수관계인 경우",
    ]

    VStack(alignment: .leading, spacing: 8) {
      Text("< 선정 절차 >")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(textColor)
        .frame(maxWidth: .infinity, alignment: .center)

      HStack(spacing: 5) {
        ForEach(Array(steps.enumerated()), id: \.offset) { idx, item in
          ZStack {
            boxFill
            VStack(spacing: 2) {
              Text("\(item.num) \(item.title)")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(textColor)
                .lineLimit(1)
              if let sub = item.sub {
                Text(sub)
                  .font(.system(size: 9, weight: .semibold))
                  .foregroundStyle(textColor.opacity(0.85))
                  .lineLimit(1)
              }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
          }
          .frame(maxWidth: .infinity, minHeight: 38, maxHeight: 38)
          .border(borderColor, width: 1)

          if idx + 1 < steps.count {
            Text("▸")
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(borderColor)
          }
        }
      }

      ZStack(alignment: .top) {
        VStack(alignment: .leading, spacing: 3) {
          ForEach(criteriaItems, id: \.self) { line in
            Text(line)
              .font(.system(size: 9.5))
              .foregroundStyle(textColor)
              .lineLimit(1)
          }
        }
        .padding(.top, 14)
        .padding(.bottom, 8)
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white)
        .border(EasyOfficeDesignTokens.resolveHexColor("#A0A0A0", fallback: .gray), width: 1)
        .padding(.top, 8)

        Text("적합성 검토 탈락기준")
          .font(.system(size: 10, weight: .bold))
          .foregroundStyle(textColor)
          .padding(.horizontal, 18)
          .padding(.vertical, 2)
          .background(boxFill)
          .border(borderColor, width: 1)
      }
    }
    .padding(.vertical, 4)
  }
}
