import SwiftUI

struct HomeTopScrollFade: View {
  var body: some View {
    GeometryReader { proxy in
      LinearGradient(
        stops: [
          .init(color: BullTheme.appBackground, location: 0),
          .init(color: BullTheme.appBackground.opacity(0.96), location: 0.56),
          .init(color: BullTheme.appBackground.opacity(0), location: 1),
        ],
        startPoint: .top,
        endPoint: .bottom
      )
      .frame(height: max(proxy.safeAreaInsets.top + 44, 82))
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      .ignoresSafeArea(edges: .top)
    }
  }
}

struct HomeStartActivityFloatingButton: View {
  @ObservedObject var session: ActivitySessionModel

  var body: some View {
    NavigationLink {
      LiveActivityView()
    } label: {
      Image(systemName: session.isActive ? session.selectedActivity.systemImage : "plus")
        .font(.system(size: 21, weight: .bold))
        .foregroundStyle(.white)
        .frame(width: 54, height: 54)
        .background(session.selectedActivity.tint, in: Circle())
        .shadow(color: .black.opacity(0.18), radius: 12, x: 0, y: 7)
        .overlay {
          Circle()
            .strokeBorder(.white.opacity(0.22), lineWidth: 1)
        }
    }
    .buttonStyle(.plain)
    .accessibilityLabel(session.isActive ? "Open Activity" : "Start Activity")
  }
}

// MARK: - Animated score ring

struct ScoreRing: View {
  let progress: Double
  let color: Color
  let lineWidth: CGFloat
  var isPlaceholder = false

  @State private var animatedProgress: Double = 0

  var body: some View {
    ZStack {
      // Track with a faint embossed inset for depth.
      Circle()
        .stroke(Color.primary.opacity(0.07), lineWidth: lineWidth)
      Circle()
        .inset(by: lineWidth * 0.78)
        .fill(Color.primary.opacity(0.025))
        .overlay {
          Circle()
            .inset(by: lineWidth * 0.78)
            .strokeBorder(Color.primary.opacity(0.05), lineWidth: 1)
        }

      if !isPlaceholder {
        Circle()
          .trim(from: 0, to: animatedProgress)
          .stroke(
            AngularGradient(
              gradient: Gradient(colors: [color.opacity(0.45), color]),
              center: .center,
              startAngle: .degrees(0),
              endAngle: .degrees(360 * max(animatedProgress, 0.001))
            ),
            style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)
          )
          .rotationEffect(.degrees(-90))
          .shadow(color: color.opacity(0.38), radius: lineWidth * 0.55)
      }
    }
    .onAppear {
      withAnimation(.spring(response: 1.05, dampingFraction: 0.88).delay(0.12)) {
        animatedProgress = progress
      }
    }
    .onChange(of: progress) { _, newValue in
      withAnimation(.spring(response: 0.7, dampingFraction: 0.9)) {
        animatedProgress = newValue
      }
    }
  }
}

// MARK: - Daily scores (Strain · Recovery · Sleep)

struct HomeScoreTriRow: View {
  let strain: HealthMetricSnapshot
  let recovery: HealthMetricSnapshot
  let sleep: HealthMetricSnapshot
  /// When set (user calibration), replaces the recovery-led verdict line.
  var calibrationVerdict: String?
  let open: (HealthRoute) -> Void

  var body: some View {
    VStack(spacing: 16) {
      HStack(alignment: .top, spacing: 12) {
        HomeScoreDial(snapshot: strain) { open(.strain) }
        HomeScoreDial(snapshot: recovery, overrideColor: recoveryColor) { open(.recovery) }
        HomeScoreDial(snapshot: sleep) { open(.sleep) }
      }
      .frame(maxWidth: .infinity)

      Text(calibrationVerdict ?? verdict)
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity)
    }
    .padding(.top, 6)
  }

  private var recoveryScore: Int? {
    guard recovery.source.kind != .unavailable,
          let value = firstNumber(in: recovery.displayValue) else {
      return nil
    }
    return min(max(Int(value.rounded()), 0), 100)
  }

  private var recoveryColor: Color? {
    guard let score = recoveryScore else { return nil }
    if score >= 67 { return .green }
    if score >= 34 { return .yellow }
    return .red
  }

  private var verdict: String {
    guard let score = recoveryScore else {
      return "Wear your band overnight to get your first Recovery score."
    }
    if score >= 67 { return "Recovered \u{2014} today can be a big day." }
    if score >= 34 { return "Getting there \u{2014} train, but keep something in reserve." }
    return "Run down \u{2014} make today about rest and recovery."
  }
}

struct HomeScoreDial: View {
  let snapshot: HealthMetricSnapshot
  var overrideColor: Color?
  let open: () -> Void

  var body: some View {
    Button(action: open) {
      VStack(spacing: 10) {
        ZStack {
          ScoreRing(
            progress: progress,
            color: dialColor,
            lineWidth: 10,
            isPlaceholder: !hasValue
          )
          HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(scoreText)
              .font(.system(size: 26, weight: .bold, design: .rounded))
              .monospacedDigit()
              .foregroundStyle(hasValue ? .primary : .secondary)
              .contentTransition(.numericText())
            if hasValue {
              Text("%")
                .font(.system(size: 14, weight: .bold, design: .rounded))
                .foregroundStyle(.secondary)
            }
          }
          .lineLimit(1)
          .minimumScaleFactor(0.6)
          .padding(12)
        }
        .frame(width: 96, height: 96)

        Text(snapshot.title)
          .font(.subheadline.weight(.semibold))
          .foregroundStyle(.primary)
          .lineLimit(1)
          .minimumScaleFactor(0.75)
      }
      .frame(maxWidth: .infinity)
    }
    .buttonStyle(.plain)
    .accessibilityElement(children: .combine)
    .accessibilityLabel(snapshot.title)
    .accessibilityValue(hasValue ? "\(scoreText) percent" : "No data yet")
  }

  private var hasValue: Bool {
    snapshot.source.kind != .unavailable && firstNumber(in: snapshot.displayValue) != nil
  }

  private var scoreText: String {
    guard hasValue else { return "--" }
    return snapshot.displayValue
      .replacingOccurrences(of: "%", with: "")
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var dialColor: Color {
    overrideColor ?? snapshot.tint
  }

  private var progress: Double {
    let value = firstNumber(in: snapshot.displayValue) ?? 0
    return min(max(value / 100, 0), 1)
  }
}

/// Converts internal / bridge status strings into calm, human language.
/// Developer screens can show raw `status`; consumer surfaces should call this.
func humanizedHomeStatus(_ status: String) -> String {
  let trimmed = status.trimmingCharacters(in: .whitespacesAndNewlines)
  guard !trimmed.isEmpty else { return "No data yet" }
  let lowered = trimmed.lowercased()

  if lowered.contains("needs whoop packet extract")
    || lowered.contains("packet extract")
    || lowered.contains("packet-derived")
    || lowered.contains("not extracted")
    || lowered.contains("extraction pending")
    || lowered.contains("rollup blocked")
    || lowered.contains("ingest blocked")
    || lowered.contains("estimator blocked")
    || lowered.contains("no run") {
    return "Sync your band — data will show up after the next sync."
  }
  if lowered.contains("whoop motion")
    || lowered.contains("counter candidates")
    || lowered.contains("daily delta pending")
    || lowered.contains("step metric pending") {
    return "Steps are still syncing from your band."
  }
  if lowered.contains("no today")
    || lowered.contains("latest stored") {
    return "Nothing for today yet — wear your band and check back."
  }
  if lowered.contains("unavailable") || lowered.contains("no data")
    || lowered.contains("no strain") || lowered.contains("no recovery")
    || lowered.contains("no sleep") || lowered.contains("waiting") {
    return "No data yet"
  }
  if lowered.contains("packet") || lowered.contains("bridge")
    || lowered.contains("decoded") || lowered.contains("metrics.") {
    return "Updating from your band…"
  }
  if lowered == "live" || lowered.hasPrefix("updated") {
    return trimmed
  }
  return trimmed
}

struct HomeStressEnergySection: View {
  let stress: HealthMetricSnapshot
  let energy: HealthMetricSnapshot
  let openStress: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HomeSectionHeader(title: "Stress & Energy")

      Button {
        openStress()
      } label: {
        HStack(spacing: 14) {
          VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
              Circle()
                .fill(stress.tint)
                .frame(width: 10, height: 10)
              Text("Today's stress")
                .font(.headline)
                .foregroundStyle(.primary)
                .lineLimit(1)
              Spacer()
            }

            Text(stress.freshness)
              .font(.caption.weight(.semibold))
              .foregroundStyle(.secondary)

            HStack(spacing: 12) {
              HomeStressStat(value: highestStressText, label: "Highest", color: .red)
              HomeStressStat(value: lowestStressText, label: "Lowest", color: .cyan)
              HomeStressStat(value: averageStressText, label: "Average", color: .green)
            }
          }

          ZStack {
            Circle()
              .stroke(stress.tint.opacity(0.14), lineWidth: 8)
            Circle()
              .trim(from: 0, to: stressProgress)
              .stroke(stress.tint, style: StrokeStyle(lineWidth: 8, lineCap: .round))
              .rotationEffect(.degrees(-90))
            VStack(spacing: 1) {
              Text(stress.value)
                .font(.title3.bold())
              Text(humanizedHomeStatus(stress.status))
                .font(.caption2.weight(.bold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
          }
          .frame(width: 76, height: 76)

          Image(systemName: "chevron.right")
            .font(.caption.weight(.bold))
            .foregroundStyle(.tertiary)
        }
        .padding(14)
        .cardSurface(tint: stress.tint, prominent: true)
      }
      .buttonStyle(.plain)

      HomeEnergyBar(
        percent: Int(firstNumber(in: energy.displayValue) ?? 0),
        caption: humanizedHomeStatus(energy.status)
      )
    }
  }

  private var stressProgress: Double {
    min(max((firstNumber(in: stress.displayValue) ?? 0) / 100, 0), 1)
  }

  private var stressValues: [Double] {
    stress.trend.points.map(\.value)
  }

  private var highestStressText: String {
    stressValues.max().map { "\(Int($0.rounded()))" } ?? "--"
  }

  private var lowestStressText: String {
    stressValues.min().map { "\(Int($0.rounded()))" } ?? "--"
  }

  private var averageStressText: String {
    firstNumber(in: stress.value).map { "\(Int($0.rounded()))" } ?? "--"
  }
}

struct HomeStressStat: View {
  let value: String
  let label: String
  let color: Color

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text(value)
        .font(.headline.bold())
        .foregroundStyle(color)
        .lineLimit(1)
        .minimumScaleFactor(0.75)
      Text(label)
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

struct HomeEnergyBar: View {
  let percent: Int
  let caption: String

  var body: some View {
    HStack(spacing: 12) {
      Image(systemName: "bolt.fill")
        .font(.system(size: 18, weight: .semibold))
        .foregroundStyle(.green)
        .frame(width: 30, height: 30)
        .background(.green.opacity(0.14), in: RoundedRectangle(cornerRadius: 10, style: .continuous))

      HStack(spacing: 3) {
        ForEach(0..<18, id: \.self) { index in
          RoundedRectangle(cornerRadius: 2, style: .continuous)
            .fill(index < filledSegments ? Color.green : Color.primary.opacity(0.12))
            .frame(height: 18)
        }
      }

      VStack(alignment: .trailing, spacing: 2) {
        Text("\(percent)%")
          .font(.headline.bold())
          .lineLimit(1)
        Text(caption)
          .font(.caption2.weight(.semibold))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }
    }
    .padding(14)
    .cardSurface(tint: .green)
  }

  private var filledSegments: Int {
    Int((Double(percent) / 100 * 18).rounded())
  }
}

struct HomeCardioLoadWidget: View {
  let snapshot: HealthMetricSnapshot
  let days: [CardioLoadDay]
  let openSheet: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HomeSectionHeader(title: "Cardio Load")

      Button(action: openSheet) {
        VStack(alignment: .leading, spacing: 16) {
          HStack(spacing: 10) {
            Image(systemName: "shoeprints.fill")
              .font(.system(size: 16, weight: .semibold))
              .foregroundStyle(.pink)
              .frame(width: 32, height: 32)
              .background(.pink.opacity(0.13), in: RoundedRectangle(cornerRadius: 10, style: .continuous))

            Text("Cardio Load")
              .font(.headline)
              .foregroundStyle(.primary)
              .lineLimit(1)

            Spacer()

            Image(systemName: "chevron.right")
              .font(.caption.weight(.bold))
              .foregroundStyle(.tertiary)
          }

          HStack(alignment: .bottom, spacing: 14) {
            VStack(alignment: .leading, spacing: 5) {
              Text(valueText)
                .font(.system(size: 34, weight: .bold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.75)

              Text(statusText)
                .font(.caption.weight(.bold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            }
            .frame(width: 96, alignment: .leading)

            HomeCardioLoadSparkline(days: days)
              .frame(height: 82)
              .frame(maxWidth: .infinity)
          }
        }
        .padding(14)
        .cardSurface(tint: .pink, prominent: true)
      }
      .buttonStyle(.plain)
      .accessibilityElement(children: .combine)
      .accessibilityLabel("Cardio Load, \(valueText), \(statusText)")
    }
  }

  private var valueText: String {
    if let latest = days.last {
      return "\(Int(latest.load.rounded()))"
    }
    return snapshot.value
  }

  private var statusText: String {
    days.last?.status ?? snapshot.status
  }
}

