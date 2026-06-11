import Darwin
import Foundation
import SwiftUI
import UIKit

struct SleepV2TrendChart: View {
  let snapshot: HealthMetricSnapshot
  let palette: SleepV2Palette
  let tint: Color

  private var points: [HealthTrendPoint] {
    snapshot.trend.points
  }

  private var values: [Double] {
    points.map(\.value)
  }

  var body: some View {
    GeometryReader { proxy in
      let domain = valueDomain
      let plot = CGRect(
        x: 50,
        y: 14,
        width: max(1, proxy.size.width - 68),
        height: max(1, proxy.size.height - 48)
      )

      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: 18, style: .continuous)
          .fill(palette.surfaceElevated.opacity(0.38))

        ForEach(0..<4, id: \.self) { tick in
          let ratio = CGFloat(tick) / 3
          let y = plot.minY + plot.height * ratio
          let value = domain.max - Double(ratio) * (domain.max - domain.min)
          Path { path in
            path.move(to: CGPoint(x: plot.minX, y: y))
            path.addLine(to: CGPoint(x: plot.maxX, y: y))
          }
          .stroke(palette.separator.opacity(tick == 3 ? 0.70 : 0.45), lineWidth: 1)

          Text(SleepV2TrendValueFormatter.format(value, snapshot: snapshot))
            .font(.caption2.weight(.semibold))
            .fontDesign(.rounded)
            .foregroundStyle(palette.mutedText)
            .frame(width: 42, alignment: .trailing)
            .position(x: 24, y: y)
        }

        if snapshot.sleepV2TrendPresentation == .bar {
          zeroLine(in: plot, domain: domain)
          barMarks(in: plot, domain: domain)
        } else {
          trendLine(in: plot, domain: domain)
            .stroke(tint, style: StrokeStyle(lineWidth: 3.5, lineCap: .round, lineJoin: .round))

          ForEach(Array(points.enumerated()), id: \.element.id) { index, point in
            let position = chartPoint(index: index, value: point.value, plot: plot, domain: domain)
            Circle()
              .fill(tint.opacity(index == points.count - 1 ? 0.22 : 0.0))
              .frame(width: 30, height: 30)
              .position(position)
            Circle()
              .fill(palette.surface)
              .frame(width: 12, height: 12)
              .overlay(Circle().stroke(tint, lineWidth: 3))
              .position(position)
          }
        }

        ForEach(xLabelIndices, id: \.self) { index in
          Text(points[index].label)
            .font(.caption2.weight(.semibold))
            .fontDesign(.rounded)
            .foregroundStyle(palette.mutedText)
            .position(x: chartPoint(index: index, value: points[index].value, plot: plot, domain: domain).x, y: plot.maxY + 22)
        }
      }
    }
  }

  private var valueDomain: (min: Double, max: Double) {
    let minValue = values.min() ?? 0
    let maxValue = values.max() ?? 1
    if snapshot.sleepV2TrendPresentation == .bar {
      let lowerBound = min(minValue, 0)
      let upperBound = max(maxValue, 0)
      let padding = max((upperBound - lowerBound) * 0.12, 1)
      return (lowerBound - padding, upperBound + padding)
    }
    let padding = max((maxValue - minValue) * 0.18, 1)
    return (minValue - padding, maxValue + padding)
  }

  private var xLabelIndices: [Int] {
    guard !points.isEmpty else {
      return []
    }
    return Array(Set([0, points.count / 2, points.count - 1])).sorted()
  }

  private func trendLine(in plot: CGRect, domain: (min: Double, max: Double)) -> Path {
    Path { path in
      for (index, point) in points.enumerated() {
        let position = chartPoint(index: index, value: point.value, plot: plot, domain: domain)
        if index == 0 {
          path.move(to: position)
        } else {
          path.addLine(to: position)
        }
      }
    }
  }

  private func zeroLine(in plot: CGRect, domain: (min: Double, max: Double)) -> some View {
    let y = yPosition(for: 0, plot: plot, domain: domain)
    return ZStack(alignment: .topLeading) {
      Path { path in
        path.move(to: CGPoint(x: plot.minX, y: y))
        path.addLine(to: CGPoint(x: plot.maxX, y: y))
      }
      .stroke(palette.text.opacity(0.24), style: StrokeStyle(lineWidth: 1.4, lineCap: .round, dash: [4, 5]))

      Text("0h")
        .font(.caption2.weight(.semibold))
        .fontDesign(.rounded)
        .foregroundStyle(palette.text.opacity(0.72))
        .frame(width: 42, alignment: .trailing)
        .position(x: 24, y: y)
    }
  }

  private func barMarks(in plot: CGRect, domain: (min: Double, max: Double)) -> some View {
    let baselineY = yPosition(for: 0, plot: plot, domain: domain)
    let barWidth = min(max(plot.width / CGFloat(max(points.count, 1)) * 0.46, 10), 28)
    return ZStack {
      ForEach(Array(points.enumerated()), id: \.element.id) { index, point in
        let x = chartPoint(index: index, value: point.value, plot: plot, domain: domain).x
        let valueY = yPosition(for: point.value, plot: plot, domain: domain)
        let height = max(6, abs(valueY - baselineY))
        RoundedRectangle(cornerRadius: 6, style: .continuous)
          .fill(barColor(for: point.value, index: index))
          .frame(width: barWidth, height: height)
          .position(x: x, y: (valueY + baselineY) / 2)
      }
    }
  }

  private func chartPoint(index: Int, value: Double, plot: CGRect, domain: (min: Double, max: Double)) -> CGPoint {
    let x = plot.minX + plot.width * CGFloat(index) / CGFloat(max(points.count - 1, 1))
    return CGPoint(x: x, y: yPosition(for: value, plot: plot, domain: domain))
  }

  private func yPosition(for value: Double, plot: CGRect, domain: (min: Double, max: Double)) -> CGFloat {
    let normalized = (value - domain.min) / max(domain.max - domain.min, 1)
    return plot.maxY - plot.height * CGFloat(normalized)
  }

  private func barColor(for value: Double, index: Int) -> Color {
    if value < 0 {
      return Color(red: 0.95, green: 0.34, blue: 0.20)
        .opacity(index == points.count - 1 ? 0.98 : 0.68)
    }
    return Color(red: 0.36, green: 0.84, blue: 0.53)
      .opacity(index == points.count - 1 ? 0.98 : 0.72)
  }
}

enum SleepV2TrendValueFormatter {
  static func format(_ value: Double, snapshot: HealthMetricSnapshot) -> String {
    if snapshot.displayValue.contains(":") {
      return clockText(value)
    }
    if snapshot.unit == "%" || snapshot.displayValue.contains("%") {
      return "\(Int(value.rounded()))%"
    }
    if snapshot.displayValue.contains("h") || shouldFormatAsMinutes(snapshot: snapshot, value: value) {
      return minutesText(value)
    }
    if !snapshot.unit.isEmpty {
      return "\(numberText(value)) \(snapshot.unit)"
    }
    return numberText(value)
  }

  private static func shouldFormatAsMinutes(snapshot: HealthMetricSnapshot, value: Double) -> Bool {
    guard snapshot.unit != "%", !snapshot.displayValue.contains("%"), !snapshot.displayValue.contains(":") else {
      return false
    }
    let title = snapshot.title.lowercased()
    return value.magnitude > 24 && (title.contains("sleep") || title.contains("asleep") || title.contains("rem") || title.contains("deep"))
  }

  private static func minutesText(_ value: Double) -> String {
    let sign = value < 0 ? "-" : ""
    let rounded = Int(value.magnitude.rounded())
    let hours = rounded / 60
    let minutes = rounded % 60
    return hours > 0 ? "\(sign)\(hours)h \(minutes)m" : "\(sign)\(minutes)m"
  }

  private static func clockText(_ value: Double) -> String {
    let hour = Int(value.rounded(.down))
    let minute = Int(((value - Double(hour)) * 60).rounded())
    return String(format: "%02d:%02d", (hour + minute / 60) % 24, minute % 60)
  }

  private static func numberText(_ value: Double) -> String {
    if value.magnitude >= 10 {
      return "\(Int(value.rounded()))"
    }
    return String(format: "%.1f", value)
  }
}

struct SleepV2TrendMetricTile: View {
  let label: String
  let value: String
  let palette: SleepV2Palette

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(label)
        .font(.caption.weight(.semibold))
        .foregroundStyle(palette.secondaryText)
      Text(value.replacingOccurrences(of: " %", with: "%"))
        .font(.subheadline.weight(.semibold))
        .fontDesign(.rounded)
        .foregroundStyle(palette.text)
        .lineLimit(1)
        .minimumScaleFactor(0.68)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(12)
    .background(palette.surfaceElevated.opacity(0.48), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
  }
}

struct SleepV2BevelTrendSelection {
  let index: Int
  let value: Double
  let date: Date
}

