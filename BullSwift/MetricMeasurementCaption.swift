import SwiftUI
import UIKit

struct MetricMeasurementCaption: View {
  let text: String
  var systemImage = "info.circle"
  var textColor = Color(.secondaryLabel)
  var iconColor: Color?

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: 6) {
      Image(systemName: systemImage)
        .font(.caption.weight(.semibold))
        .foregroundStyle(iconColor ?? textColor)
        .accessibilityHidden(true)
      Text(text)
        .font(.caption.weight(.medium))
        .foregroundStyle(textColor)
        .fixedSize(horizontal: false, vertical: true)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
  }

  static func calibration(
    _ title: String,
    observed: Int,
    required: Int,
    unit: String,
    systemImage: String = "hourglass"
  ) -> MetricMeasurementCaption {
    let clampedObserved = min(max(observed, 0), required)
    return MetricMeasurementCaption(
      text: "\(title): \(clampedObserved) of \(required) \(unit)",
      systemImage: systemImage
    )
  }
}

enum MetricMeasurementCopy {
  static let recoveryWindow = "Set against your last 30 nights"
  static let sleepScoredFromLastNight = "Scored from last night"
  static let sleepCoachNeedsThreeNights = "3 nights needed for personalized bed & wake times"
  static let strainToday = "Reflects today"
  static let stressToday = "Reflects today"
  static let selectedDay = "Reflects selected day"
  static let hrvRhrTypicalRange = "Typical range over the last 30 days (25th–75th percentile)"
}
