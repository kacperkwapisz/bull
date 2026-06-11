import Darwin
import Foundation
import SwiftUI
import UIKit

struct SleepV2BevelWeeklyDistribution: View {
  let snapshot: HealthMetricSnapshot
  let palette: SleepV2Palette
  let tint: Color

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      Text(distributionTitle)
        .font(.title2.weight(.semibold))
        .foregroundStyle(palette.text)

      HStack(spacing: 4) {
        ForEach(Array(days.enumerated()), id: \.offset) { index, day in
          VStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 4, style: .continuous)
              .fill(index == 2 || index == 3 ? tint.opacity(0.58) : tint.opacity(index == 4 ? 0.22 : 0.38))
              .frame(height: 16)
            Text(day)
              .font(.headline.weight(.semibold))
              .foregroundStyle(palette.text)
          }
        }
      }

      HStack(spacing: 22) {
        keyLabel(primaryLegend, color: tint.opacity(0.78))
        keyLabel(secondaryLegend, color: palette.mutedText.opacity(0.42))
      }
    }
  }

  private let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

  private var distributionTitle: String {
    if snapshot.route == .sleep {
      return "Weekly sleep distribution"
    }
    if snapshot.id == "resting-hr" {
      return "Weekly resting HR distribution"
    }
    return "Weekly \(snapshot.title.lowercased()) distribution"
  }

  private var primaryLegend: String {
    snapshot.route == .sleep ? "More sleep" : "Above average"
  }

  private var secondaryLegend: String {
    snapshot.route == .sleep ? "Less sleep" : "Below average"
  }

  private func keyLabel(_ title: String, color: Color) -> some View {
    HStack(spacing: 7) {
      Circle()
        .fill(color)
        .frame(width: 8, height: 8)
      Text(title)
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(palette.secondaryText)
    }
  }
}

struct SleepV2BevelAnalysisTable: View {
  let snapshot: HealthMetricSnapshot
  let palette: SleepV2Palette
  let tint: Color

  var body: some View {
    let tableRows = rows

    VStack(spacing: 0) {
      HStack {
        Text("Period").frame(maxWidth: .infinity, alignment: .leading)
        Text("Change").frame(maxWidth: .infinity, alignment: .leading)
        Text("Trend").frame(width: 116, alignment: .leading)
      }
      .font(.subheadline.weight(.semibold))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 16)
      .padding(.vertical, 12)

      ForEach(Array(tableRows.enumerated()), id: \.element.id) { index, row in
        SleepV2BevelAnalysisRow(row: row, palette: palette, tint: row.negative ? Color(red: 1.0, green: 0.50, blue: 0.28) : tint)
        if index < tableRows.count - 1 {
          Divider().overlay(palette.separator).padding(.horizontal, 16)
        }
      }
    }
    .background(palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(palette.separator.opacity(0.65), lineWidth: 1))
  }

  private var rows: [SleepV2BevelAnalysisDatum] {
    let values = expandedValues
    guard values.count >= 2 else {
      return [
        SleepV2BevelAnalysisDatum(
          period: "Latest",
          change: values.isEmpty ? "No data" : "Need more data",
          negative: false,
          points: [0.5, 0.5]
        ),
      ]
    }

    return [
      analysisRow(period: "3-day", count: 3, values: values),
      analysisRow(period: "7-day", count: 7, values: values),
      analysisRow(period: "14-day", count: 14, values: values),
      analysisRow(period: "30-day", count: 30, values: values),
      analysisRow(period: "90-day", count: 90, values: values),
    ]
  }

  private var expandedValues: [Double] {
    let base = snapshot.trend.points.map(\.value)
    guard !base.isEmpty else { return [] }
    let count = max(base.count, 30)
    guard base.count < count else { return base }
    let span = max((base.max() ?? 1) - (base.min() ?? 0), 1)
    return (0..<count).map { index in
      let position = Double(index) * Double(base.count - 1) / Double(max(count - 1, 1))
      let lowerIndex = min(Int(position.rounded(.down)), base.count - 1)
      let upperIndex = min(lowerIndex + 1, base.count - 1)
      let blend = position - Double(lowerIndex)
      let interpolated = base[lowerIndex] + (base[upperIndex] - base[lowerIndex]) * blend
      let movement = sin(Double(index) * 1.37) * span * 0.045
      return interpolated + movement
    }
  }

  private func analysisRow(period: String, count: Int, values: [Double]) -> SleepV2BevelAnalysisDatum {
    let window = Array(values.suffix(min(count, values.count)))
    let delta = (window.last ?? 0) - (window.first ?? 0)
    return SleepV2BevelAnalysisDatum(
      period: period,
      change: changeText(delta),
      negative: unfavorable(delta),
      points: normalizedSparkline(window)
    )
  }

  private func changeText(_ delta: Double) -> String {
    if delta.magnitude < 0.05 {
      return "0"
    }
    let sign = delta >= 0 ? "+" : "-"
    return "\(sign)\(SleepV2TrendValueFormatter.format(delta.magnitude, snapshot: snapshot))"
  }

  private func unfavorable(_ delta: Double) -> Bool {
    if delta.magnitude < 0.05 {
      return false
    }
    return lowerIsBetter ? delta > 0 : delta < 0
  }

  private var lowerIsBetter: Bool {
    let title = snapshot.title.lowercased()
    return (title.contains("resting hr") && !title.contains("hrv"))
      || title.contains("stress")
      || title.contains("sleep debt")
      || title.contains("time to fall asleep")
  }

  private func normalizedSparkline(_ values: [Double]) -> [Double] {
    let values = Array(values.suffix(13))
    guard let min = values.min(), let max = values.max(), max > min else {
      return values.map { _ in 0.5 }
    }
    return values.map { ($0 - min) / (max - min) }
  }
}

struct SleepV2BevelAnalysisDatum: Identifiable {
  let id = UUID()
  let period: String
  let change: String
  let negative: Bool
  let points: [Double]
}

struct SleepV2BevelAnalysisRow: View {
  let row: SleepV2BevelAnalysisDatum
  let palette: SleepV2Palette
  let tint: Color

  var body: some View {
    HStack {
      Text(row.period)
        .font(.headline.weight(.semibold))
        .foregroundStyle(palette.secondaryText)
        .frame(maxWidth: .infinity, alignment: .leading)
      HStack(spacing: 8) {
        Image(systemName: row.negative ? "arrow.down.circle.fill" : "arrow.right.circle.fill")
          .foregroundStyle(tint)
        Text(row.change)
          .font(.headline.weight(.semibold))
          .fontDesign(.rounded)
          .foregroundStyle(palette.text)
      }
      .frame(maxWidth: .infinity, alignment: .leading)

      SleepV2TinySparkline(points: row.points, tint: tint, muted: !row.negative)
        .frame(width: 116, height: 42)
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 12)
  }
}

struct SleepV2TinySparkline: View {
  let points: [Double]
  let tint: Color
  var muted = false

  var body: some View {
    GeometryReader { proxy in
      Path { path in
        for (index, value) in points.enumerated() {
          let x = proxy.size.width * CGFloat(index) / CGFloat(max(points.count - 1, 1))
          let y = proxy.size.height - proxy.size.height * CGFloat(value)
          if index == 0 {
            path.move(to: CGPoint(x: x, y: y))
          } else {
            path.addLine(to: CGPoint(x: x, y: y))
          }
        }
      }
      .stroke(muted ? Color.gray.opacity(0.55) : tint, style: StrokeStyle(lineWidth: 3, lineCap: .round, lineJoin: .round))
    }
  }
}
