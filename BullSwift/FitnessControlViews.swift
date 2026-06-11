import CoreLocation
import MapKit
import SwiftUI
import UIKit

struct FitnessMetricPageLayout<Content: View>: View {
  private let content: Content

  init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  var body: some View {
    content
      .padding(.horizontal, 22)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}

struct FitnessControlDock: View {
  let activity: ActivityKind
  let elapsed: TimeInterval
  let isActive: Bool
  let isPaused: Bool
  let segmentNumber: Int
  @Binding var expanded: Bool
  let controlsLocked: Bool
  let onPrimaryAction: () -> Void
  let onEndWorkout: () -> Void
  let onStopViewing: () -> Void
  let onLockControls: () -> Void
  let onUnlockControls: () -> Void
  let onActivityTap: () -> Void
  let onSegmentTap: () -> Void
  let onHeartPageTap: () -> Void

  var body: some View {
    ZStack(alignment: .top) {
      UnevenRoundedRectangle(
        topLeadingRadius: 58,
        bottomLeadingRadius: expanded ? 0 : 58,
        bottomTrailingRadius: expanded ? 0 : 58,
        topTrailingRadius: 58,
        style: .continuous
      )
        .fill(FitnessColor.panel)

      Capsule()
        .fill(FitnessColor.grabber)
        .frame(width: 42, height: 6)
        .padding(.top, 10)

      if expanded {
        expandedControls
      } else {
        compactControls
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .animation(dockAnimation, value: expanded)
  }

  private var compactControls: some View {
    VStack(spacing: 24) {
      HStack(alignment: .center, spacing: 16) {
        Button(action: onActivityTap) {
          FitnessWorkoutIcon(activity: activity, size: 48, backgroundOpacity: 0.32)
            .frame(width: 48, height: 48)
        }
        .buttonStyle(.plain)
        .disabled(controlsLocked)

        FitnessDockTimerText(elapsed: elapsed, size: 52, color: FitnessColor.workoutYellow, width: 218)
          .frame(maxWidth: .infinity)
          .onTapGesture {
            if !controlsLocked {
              expandDock()
            }
          }

        Color.clear
          .frame(width: 48, height: 48)
      }
      .padding(.horizontal, 24)
      .padding(.top, 38)

      HStack(alignment: .center) {
        Button(action: onSegmentTap) {
          FitnessSegmentBadge(number: segmentNumber, size: 72)
            .frame(width: 86, height: 86)
            .background(FitnessColor.controlButton, in: Circle())
        }
        .buttonStyle(.plain)
        .disabled(!isActive || controlsLocked)

        Spacer()

        Button {
          if !controlsLocked {
            onPrimaryAction()
          }
        } label: {
          ZStack(alignment: .topTrailing) {
            Image(systemName: primaryIcon)
              .font(.system(size: 54, weight: .medium))
              .foregroundStyle(.white)
              .frame(width: 122, height: 122)
              .background(FitnessColor.controlButton, in: Circle())
            if controlsLocked {
              Image(systemName: "lock.fill")
                .font(.system(size: 19, weight: .bold))
                .foregroundStyle(.black)
                .frame(width: 36, height: 36)
                .background(FitnessColor.lime, in: Circle())
                .offset(x: -2, y: 0)
            }
          }
        }
        .buttonStyle(.plain)
        .simultaneousGesture(
          LongPressGesture(minimumDuration: 5, maximumDistance: 42)
            .onEnded { _ in
              if controlsLocked {
                onUnlockControls()
              }
            }
        )

        Spacer()

        Button(action: onHeartPageTap) {
          ZStack(alignment: .bottomTrailing) {
            Image(systemName: "waveform.path.ecg")
              .font(.system(size: 36, weight: .semibold))
              .foregroundStyle(.white)
              .frame(width: 86, height: 86)
              .background(FitnessColor.controlButton, in: Circle())
              .overlay(alignment: .topTrailing) {
                Image(systemName: "heart.fill")
                  .font(.system(size: 18, weight: .bold))
                  .foregroundStyle(.white)
                  .offset(x: -17, y: 19)
              }
            Text("2")
              .font(.system(size: 18, weight: .bold, design: .rounded))
              .foregroundStyle(.white)
              .frame(width: 38, height: 38)
              .background(FitnessColor.badge, in: Circle())
              .offset(x: 8, y: 7)
          }
        }
        .buttonStyle(.plain)
        .disabled(controlsLocked)
      }
      .padding(.horizontal, 22)
    }
  }

  private var expandedControls: some View {
    VStack(spacing: 20) {
      HStack(alignment: .center, spacing: 16) {
        FitnessWorkoutIcon(activity: activity, size: 50, backgroundOpacity: 0.22)
        FitnessDockTimerText(elapsed: elapsed, size: 50, color: FitnessColor.workoutYellow.opacity(0.45), width: 212)
        Spacer()
      }
      .padding(.top, 38)
      .padding(.horizontal, 24)

      HStack(spacing: 34) {
        Button(action: onSegmentTap) {
          FitnessSegmentBadge(number: segmentNumber, size: 68)
            .frame(width: 82, height: 82)
            .background(FitnessColor.controlButton.opacity(0.5), in: Circle())
        }
        .buttonStyle(.plain)

        Button {
          collapseDock()
          onPrimaryAction()
        } label: {
          Image(systemName: "arrow.clockwise")
            .font(.system(size: 50, weight: .medium))
            .foregroundStyle(FitnessColor.workoutYellow)
            .frame(width: 116, height: 116)
            .background(FitnessColor.workoutYellow.opacity(0.15), in: Circle())
        }

        ZStack(alignment: .bottomTrailing) {
          Image(systemName: "waveform.path.ecg")
            .font(.system(size: 34, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: 82, height: 82)
            .background(FitnessColor.controlButton.opacity(0.5), in: Circle())
          Text("2")
            .font(.system(size: 18, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: 36, height: 36)
            .background(FitnessColor.badge, in: Circle())
            .offset(x: 8, y: 7)
        }
      }

      VStack(spacing: 16) {
        FitnessExpandedControlButton(
          title: "End Workout",
          systemImage: "xmark",
          foreground: FitnessColor.endRed,
          background: FitnessColor.endRed.opacity(0.22),
          action: onEndWorkout
        )

        FitnessExpandedControlButton(
          title: "Stop Viewing",
          systemImage: "iphone.and.play",
          foreground: .white,
          background: FitnessColor.controlButton,
          action: onStopViewing
        )

        FitnessExpandedControlButton(
          title: "Lock Controls",
          systemImage: "lock.fill",
          foreground: .white,
          background: FitnessColor.controlButton,
          action: onLockControls
        )
      }
      .padding(.horizontal, 18)
      .padding(.bottom, 26)
    }
  }

  private var primaryIcon: String {
    if isActive && isPaused {
      return "play.fill"
    }
    if isActive {
      return "pause.fill"
    }
    return "play.fill"
  }

  private var dockAnimation: Animation {
    .interactiveSpring(response: 0.44, dampingFraction: 0.9, blendDuration: 0.12)
  }

  private func expandDock() {
    withAnimation(dockAnimation) {
      expanded = true
    }
  }

  private func collapseDock() {
    withAnimation(dockAnimation) {
      expanded = false
    }
  }
}

struct FitnessDockTimerText: View {
  let elapsed: TimeInterval
  let size: CGFloat
  let color: Color
  let width: CGFloat

  var body: some View {
    Text(formatFitnessDockDuration(elapsed))
      .font(.system(size: size, weight: .semibold, design: .rounded))
      .monospacedDigit()
      .contentTransition(.numericText(value: elapsed))
      .foregroundStyle(color)
      .lineLimit(1)
      .minimumScaleFactor(0.64)
      .frame(width: width, alignment: .center)
      .transaction { transaction in
        transaction.animation = nil
      }
  }
}

struct FitnessExpandedControlButton: View {
  let title: String
  let systemImage: String
  let foreground: Color
  let background: Color
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      Label(title, systemImage: systemImage)
        .font(.system(size: 28, weight: .semibold, design: .rounded))
        .foregroundStyle(foreground)
        .frame(maxWidth: .infinity)
        .frame(height: 80)
        .background(background, in: Capsule())
    }
    .buttonStyle(.plain)
  }
}

struct FitnessPageDots: View {
  let activity: ActivityKind
  let selectedPage: FitnessWorkoutPage

  var body: some View {
    HStack(spacing: 8) {
      ForEach(FitnessWorkoutPage.pages(for: activity)) { page in
        Circle()
          .fill(page == selectedPage ? Color.white : FitnessColor.pageDot)
          .frame(width: 8, height: 8)
      }
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 8)
  }
}

struct FitnessCountdownView: View {
  let value: Int
  let activity: ActivityKind
  let onSkip: () -> Void
  @State private var ringProgress: CGFloat = 1

  var body: some View {
    GeometryReader { proxy in
      let center = CGPoint(x: proxy.size.width / 2, y: proxy.size.height * 0.48)

      ZStack {
        FitnessWorkoutIcon(activity: activity, size: 66, backgroundOpacity: 0.36)
          .position(x: center.x, y: center.y - 194)

        ZStack {
          Circle()
            .stroke(FitnessColor.exerciseGreen.opacity(0.28), lineWidth: 18)
          Circle()
            .trim(from: 0, to: ringProgress)
            .stroke(
              LinearGradient(colors: [FitnessColor.exerciseGreen, FitnessColor.lime], startPoint: .top, endPoint: .bottom),
              style: StrokeStyle(lineWidth: 18, lineCap: .round)
            )
            .rotationEffect(.degrees(-90))
          Text("\(value)")
            .font(.system(size: 82, weight: .regular, design: .rounded))
            .foregroundStyle(.white)
        }
        .frame(width: 258, height: 258)
        .position(center)

        Text(activity.fitnessTitle)
          .font(.system(size: 34, weight: .regular, design: .rounded))
          .foregroundStyle(.white)
          .position(x: center.x, y: center.y + 188)
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .contentShape(Rectangle())
      .onTapGesture(perform: onSkip)
      .onAppear {
        animateRing(from: CGFloat(value) / 3)
      }
      .onChange(of: value) { _, newValue in
        animateRing(from: CGFloat(newValue) / 3)
      }
    }
  }

  private func animateRing(from progress: CGFloat) {
    ringProgress = progress
    withAnimation(.linear(duration: 0.96)) {
      ringProgress = max(progress - (1.0 / 3.0), 0)
    }
  }
}

struct FitnessActivityPickerStartView: View {
  let selectedActivity: ActivityKind
  let recentActivities: [ActivityKind]
  let onStart: (ActivityKind) -> Void

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: recentActivities.isEmpty ? 10 : 24) {
        if recentActivities.isEmpty {
          workoutRows(activities: ActivityKind.allCases)
        } else {
          workoutSection(title: "Recently Used", activities: recentActivities)
          workoutSection(title: "All Workouts", activities: ActivityKind.allCases)
        }
      }
      .padding(.horizontal, 18)
      .padding(.top, 12)
      .padding(.bottom, 32)
    }
    .scrollIndicators(.hidden)
    .background(FitnessColor.background)
  }

  private func workoutSection(title: String, activities: [ActivityKind]) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text(title)
        .font(.system(size: 13, weight: .bold, design: .rounded))
        .foregroundStyle(FitnessColor.secondaryText)
        .textCase(.uppercase)
        .padding(.horizontal, 4)

      workoutRows(activities: activities)
    }
  }

  private func workoutRows(activities: [ActivityKind]) -> some View {
    VStack(spacing: 10) {
      ForEach(activities) { activity in
        FitnessActivityStartRow(
          activity: activity,
          isSelected: selectedActivity == activity
        ) {
          onStart(activity)
        }
      }
    }
  }
}

struct FitnessActivityStartRow: View {
  let activity: ActivityKind
  let isSelected: Bool
  let onStart: () -> Void

  var body: some View {
    Button(action: onStart) {
      HStack(spacing: 14) {
        FitnessWorkoutIcon(activity: activity, size: 48, backgroundOpacity: isSelected ? 0.34 : 0.22)

        VStack(alignment: .leading, spacing: 3) {
          Text(activity.fitnessTitle)
            .font(.system(size: 19, weight: .semibold, design: .rounded))
            .foregroundStyle(.white)
          Text(activity.subtitle)
            .font(.system(size: 14, weight: .medium, design: .rounded))
            .foregroundStyle(FitnessColor.secondaryText)
        }

        Spacer(minLength: 12)

        Text("Start")
          .font(.system(size: 15, weight: .bold, design: .rounded))
          .foregroundStyle(.black)
          .padding(.horizontal, 14)
          .frame(height: 32)
          .background(FitnessColor.lime, in: Capsule())
      }
      .padding(.horizontal, 14)
      .padding(.vertical, 13)
      .background(
        FitnessColor.panel,
        in: RoundedRectangle(cornerRadius: 26, style: .continuous)
      )
      .overlay {
        if isSelected {
          RoundedRectangle(cornerRadius: 26, style: .continuous)
            .stroke(FitnessColor.lime.opacity(0.42), lineWidth: 1)
        }
      }
    }
    .buttonStyle(.plain)
  }
}

struct FitnessActivityPickerSheet: View {
  @Environment(\.dismiss) private var dismiss
  let selectedActivity: ActivityKind
  let recentActivities: [ActivityKind]
  let onSelect: (ActivityKind) -> Void

  var body: some View {
    NavigationStack {
      List {
        if !recentActivities.isEmpty {
          Section("Recently Used") {
            ForEach(recentActivities) { activity in
              workoutPickerButton(activity)
            }
          }
          Section("All Workouts") {
            ForEach(ActivityKind.allCases) { activity in
              workoutPickerButton(activity)
            }
          }
        } else {
          ForEach(ActivityKind.allCases) { activity in
            workoutPickerButton(activity)
          }
        }
      }
      .scrollContentBackground(.hidden)
      .background(FitnessColor.background)
      .navigationTitle("Workout")
      .navigationBarTitleDisplayMode(.large)
      .toolbarColorScheme(.dark, for: .navigationBar)
      .toolbarBackground(FitnessColor.background, for: .navigationBar)
      .toolbarBackground(.visible, for: .navigationBar)
    }
  }

  private func workoutPickerButton(_ activity: ActivityKind) -> some View {
    Button {
      onSelect(activity)
      dismiss()
    } label: {
      HStack(spacing: 14) {
        FitnessWorkoutIcon(activity: activity, size: 44, backgroundOpacity: 0.22)
        VStack(alignment: .leading, spacing: 2) {
          Text(activity.fitnessTitle)
            .foregroundStyle(.white)
          Text(activity.subtitle)
            .font(.caption)
            .foregroundStyle(FitnessColor.secondaryText)
        }
        Spacer()
        if selectedActivity == activity {
          Image(systemName: "checkmark.circle.fill")
            .foregroundStyle(FitnessColor.exerciseGreen)
        }
      }
    }
    .listRowBackground(FitnessColor.panel)
  }
}

