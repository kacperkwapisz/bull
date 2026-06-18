import SwiftUI
import UIKit

struct DeviceView: View {
  @EnvironmentObject private var model: BullAppModel

  var body: some View {
    DeviceContentView(ble: model.ble)
      .environmentObject(model)
  }
}

private enum DevicePanel {
  case status
  case advanced
}

private struct DeviceContentView: View {
  @EnvironmentObject private var model: BullAppModel
  @EnvironmentObject private var packetMonitor: PacketMonitorModel
  @EnvironmentObject private var calibration: CalibrationManager
  @ObservedObject var ble: BullBLEClient
  @State private var selectedPanel: DevicePanel = .status

  var body: some View {
    ZStack {
      deviceScreenBackground.ignoresSafeArea()
      ScrollView {
        VStack(alignment: .leading, spacing: 0) {
          DeviceConnectionHeader(
            connected: deviceConnected,
            statusText: connectionHeadline,
            deviceName: ble.activeDeviceName,
            lastSync: lastSyncSummary
          )
          .padding(.bottom, 30)

          DeviceStatusTabs(selectedPanel: $selectedPanel)
            .padding(.bottom, 46)

          if selectedPanel == .status {
            DeviceImageAndBattery(
              batteryPercent: ble.batteryLevelPercent,
              isCharging: ble.batteryIsCharging == true
            )
            DeviceBatteryPackTile(ble: ble)
              .padding(.top, 10)
          } else {
            DeviceAdvancedPanel(model: model, packetMonitor: packetMonitor, ble: ble)
          }
        }
        .padding(.horizontal, 22)
        .padding(.top, 36)
        .padding(.bottom, 28)
      }
    }
    .navigationTitle("Device")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .tint(devicePrimaryText)
    .toolbar {
      ToolbarItem(placement: .topBarTrailing) {
        Button {
          ble.refreshBatteryLevel()
          ble.refreshDeviceInformation()
          ble.requestBatteryPackInfo(reason: "toolbar_refresh")
        } label: {
          Image(systemName: "battery.75percent")
        }
        .foregroundStyle(devicePrimaryText)
        .accessibilityLabel("Refresh Device")
      }
    }
    .onAppear {
      ble.refreshBatteryLevel()
      ble.refreshDeviceInformation()
      ble.requestBatteryPackInfo(reason: "device_view_appear")
      calibration.ensureStarted(connectedAt: ble.connectedAt)
    }
    .onChange(of: ble.connectedAt) { _, connectedAt in
      calibration.ensureStarted(connectedAt: connectedAt)
    }
    .task {
      while !Task.isCancelled {
        ble.refreshBatteryLevel()
        try? await Task.sleep(for: .seconds(60))
      }
    }
  }

  private var deviceConnected: Bool {
    let state = ble.connectionState.lowercased()
    return state == "ready" || state == "connected" || state == "discovering"
  }

  private var connectionHeadline: String {
    let state = ble.connectionState.lowercased()
    if deviceConnected {
      return "CONNECTED"
    }
    if state == "connecting" {
      return "CONNECTING"
    }
    if ble.isScanning {
      return "SCANNING"
    }
    return "NOT CONNECTED"
  }

  private var lastSyncSummary: String {
    relativeSummary(for: ble.lastSyncAt) ?? "Not synced"
  }
}

private struct DeviceStatusTabs: View {
  @Binding var selectedPanel: DevicePanel

  var body: some View {
    HStack(spacing: 46) {
      DeviceTabButton(
        label: "STATUS",
        selected: selectedPanel == .status
      ) {
        withAnimation(.easeOut(duration: 0.16)) {
          selectedPanel = .status
        }
      }
      DeviceTabButton(
        label: "ADVANCED",
        selected: selectedPanel == .advanced
      ) {
        withAnimation(.easeOut(duration: 0.16)) {
          selectedPanel = .advanced
        }
      }
    }
  }
}

private struct DeviceTabButton: View {
  let label: String
  let selected: Bool
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      VStack(alignment: .leading, spacing: 10) {
        Text(label)
          .font(deviceLabelFont)
          .foregroundStyle(selected ? devicePrimaryText : mutedText)
        Rectangle()
          .fill(devicePrimaryText)
          .frame(width: selected ? underlineWidth : 0, height: 3)
      }
      .frame(width: label == "ADVANCED" ? 96 : 72, alignment: .leading)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
  }

  private var underlineWidth: CGFloat {
    label == "ADVANCED" ? 76 : 52
  }
}

private struct DeviceImageAndBattery: View {
  let batteryPercent: Int?
  let isCharging: Bool

  var body: some View {
    GeometryReader { proxy in
      let imageWidth = min(max(proxy.size.width * 0.95, 290), 390)
      let percentFontSize = min(max(proxy.size.width * 0.155, 50), 62)
      ZStack(alignment: .topLeading) {
        Image("whoop_gen5_front")
          .resizable()
          .scaledToFit()
          .frame(width: imageWidth, height: 305)
          .offset(x: -imageWidth * 0.28, y: 36)
          .accessibilityLabel("WHOOP strap")

        HStack(alignment: .bottom, spacing: 18) {
          HStack(alignment: .bottom, spacing: 0) {
            Text(batteryText)
              .font(.system(size: percentFontSize, weight: .black, design: .default))
              .foregroundStyle(devicePrimaryText)
              .lineLimit(1)
              .minimumScaleFactor(0.7)
            Text("%")
              .font(.system(size: percentFontSize * 0.42, weight: .black, design: .default))
              .foregroundStyle(devicePrimaryText)
              .padding(.bottom, percentFontSize * 0.08)
          }
          BatteryRail(percent: batteryPercent, isCharging: isCharging)
        }
        .frame(maxWidth: proxy.size.width, alignment: .trailing)
        .padding(.top, 190)
      }
      .frame(width: proxy.size.width, height: 350, alignment: .topLeading)
    }
    .frame(height: 350)
  }

  private var batteryText: String {
    guard let batteryPercent else {
      return "--"
    }
    return "\(batteryPercent)"
  }
}

private struct DeviceConnectionHeader: View {
  let connected: Bool
  let statusText: String
  let deviceName: String
  let lastSync: String

  var body: some View {
    HStack(alignment: .bottom, spacing: 16) {
      VStack(alignment: .leading, spacing: 7) {
        Text(statusText)
          .font(deviceLabelFont)
          .foregroundStyle(connected ? connectedGreen : disconnectedRed)
          .lineLimit(1)
        Text(deviceName.uppercased())
          .font(.system(size: 26, weight: .black, design: .default))
          .foregroundStyle(devicePrimaryText)
          .lineLimit(2)
          .minimumScaleFactor(0.78)
      }
      .frame(maxWidth: .infinity, alignment: .leading)

      VStack(alignment: .trailing, spacing: 7) {
        Text("LAST SYNC")
          .font(deviceLabelFont)
          .foregroundStyle(secondaryText)
        HStack(spacing: 8) {
          Text(lastSync)
            .font(deviceBodyFont.weight(.black))
            .foregroundStyle(devicePrimaryText)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
          Image(systemName: "icloud")
            .font(.system(size: 24, weight: .regular))
            .foregroundStyle(secondaryText)
        }
      }
    }
  }
}

private struct BatteryRail: View {
  let percent: Int?
  let isCharging: Bool
  @State private var chargingPulse = false

  var body: some View {
    ZStack(alignment: .bottom) {
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(deviceRailBackground)
        .frame(width: 10, height: 138)
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(fillStyle)
        .frame(width: 10, height: 138 * CGFloat(value))
        .opacity(isCharging ? (chargingPulse ? 1 : 0.62) : 1)
        .shadow(color: isCharging ? batteryYellow.opacity(chargingPulse ? 0.7 : 0.18) : .clear, radius: chargingPulse ? 10 : 2)
      if isCharging {
        Image(systemName: "bolt.fill")
          .font(.system(size: 15, weight: .black))
          .foregroundStyle(batteryYellow)
          .shadow(color: batteryYellow.opacity(0.55), radius: chargingPulse ? 8 : 2)
          .scaleEffect(chargingPulse ? 1.12 : 0.92)
          .offset(y: -150)
          .accessibilityHidden(true)
      }
    }
    .frame(width: 12, height: 138)
    .onAppear {
      chargingPulse = isCharging
    }
    .onChange(of: isCharging) { _, charging in
      chargingPulse = charging
    }
    .animation(
      isCharging ? .easeInOut(duration: 0.9).repeatForever(autoreverses: true) : .default,
      value: chargingPulse
    )
  }

  private var value: Double {
    Double(min(max(percent ?? 0, 0), 100)) / 100
  }

  private var fillStyle: LinearGradient {
    LinearGradient(
      colors: isCharging
        ? [batteryYellow, Color(red: 0.74, green: 1.0, blue: 0.56), batteryYellow]
        : [batteryYellow, batteryYellow],
      startPoint: .bottom,
      endPoint: .top
    )
  }
}

// Compact horizontal battery level indicator for the battery pack tile
// (visual parity with BatteryRail used for the main strap battery).
private struct PackBatteryLevel: View {
  let percent: Int
  let isLow: Bool
  let isCharging: Bool

  var body: some View {
    ZStack(alignment: .leading) {
      RoundedRectangle(cornerRadius: 2, style: .continuous)
        .fill(deviceRailBackground)
        .frame(width: 36, height: 8)
      RoundedRectangle(cornerRadius: 2, style: .continuous)
        .fill(fillColor)
        .frame(width: 36 * CGFloat(min(max(percent, 0), 100)) / 100, height: 8)
    }
    .accessibilityHidden(true)
  }

  private var fillColor: Color {
    if isLow { return disconnectedRed }
    if isCharging {
      return batteryYellow // solid; gradient unnecessary at this size
    }
    return batteryYellow
  }
}

private struct DeviceBatteryPackTile: View {
  @ObservedObject var ble: BullBLEClient

  private var present: Bool { ble.batteryPackPresent == true }
  private var strapIsCharging: Bool { ble.batteryIsCharging == true }
  private var artBase: String {
    ble.batteryPackType == .penguin ? "PackPenguin" : "PackPuffin"
  }

  var body: some View {
    HStack(spacing: 16) {
      ZStack {
        if present && strapIsCharging {
          PackFrameSequence(prefix: "\(artBase)Frame", frameCount: 25)
            .frame(width: 64, height: 73)
        } else {
          Image("\(artBase)Glyph")
            .resizable()
            .scaledToFit()
            .frame(width: 64, height: 73)
            .opacity(present ? 1 : 0.35)
        }
      }
      .frame(width: 64, height: 73)

      VStack(alignment: .leading, spacing: 5) {
        Text("BATTERY PACK")
          .font(deviceLabelFont)
          .foregroundStyle(secondaryText)
        if present, let percent = ble.batteryPackPercent {
          HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("\(percent)%")
              .font(.system(size: 28, weight: .black, design: .default))
              .foregroundStyle(ble.batteryPackIsLow ? disconnectedRed : devicePrimaryText)
            PackBatteryLevel(percent: percent, isLow: ble.batteryPackIsLow, isCharging: strapIsCharging)
            if strapIsCharging {
              Image(systemName: "bolt.fill")
                .font(.system(size: 14, weight: .black))
                .foregroundStyle(batteryYellow)
                .accessibilityHidden(true)
            }
          }
          Text(subtitle)
            .font(deviceBodyFont)
            .foregroundStyle(secondaryText)
            .lineLimit(1)
            .minimumScaleFactor(0.8)
        } else {
          Text("Attach your battery pack to see its charge")
            .font(deviceBodyFont)
            .foregroundStyle(secondaryText)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
      Spacer(minLength: 0)
    }
    .padding(16)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 18, style: .continuous)
        .fill(deviceRailBackground.opacity(0.45))
    )
    .accessibilityElement(children: .combine)
    .accessibilityLabel(accessibilitySummary)
  }

  private var subtitle: String {
    var parts: [String] = []
    let typeName = ble.batteryPackType.displayName
    if !typeName.isEmpty {
      parts.append(typeName)
    }
    if ble.batteryPackIsLow {
      parts.append("Low")
    } else if strapIsCharging {
      parts.append("Charging strap")
    }
    if let updatedAt = ble.batteryPackUpdatedAt, Date().timeIntervalSince(updatedAt) > 3600 {
      parts.append("stale")
    }
    return parts.isEmpty ? "Connected" : parts.joined(separator: " \u{00B7} ")
  }

  private var accessibilitySummary: String {
    guard present, let percent = ble.batteryPackPercent else {
      return "Battery pack not attached"
    }
    let charging = strapIsCharging ? ", charging the strap" : ""
    return "Battery pack \(percent) percent\(charging)"
  }
}

// Plays a WHOOP battery-pack charging animation as a native frame sequence
// (the source .lottie assets are 25-frame, 15fps image sequences).
private struct PackFrameSequence: View {
  let prefix: String
  let frameCount: Int
  @State private var index = 0
  private let timer = Timer.publish(every: 1.0 / 15.0, on: .main, in: .common).autoconnect()

  var body: some View {
    Image("\(prefix)\(String(format: "%02d", index))")
      .resizable()
      .scaledToFit()
      .onReceive(timer) { _ in
        index = frameCount > 0 ? (index + 1) % frameCount : 0
      }
      .accessibilityHidden(true)
  }
}

private struct DeviceAdvancedPanel: View {
  @EnvironmentObject private var messageStore: BullMessageStore
  @ObservedObject var model: BullAppModel
  @ObservedObject var packetMonitor: PacketMonitorModel
  @ObservedObject var ble: BullBLEClient

  var body: some View {
    VStack(alignment: .leading, spacing: 22) {
      DeviceDetailStack {
        DeviceFactRow(systemName: "gearshape", label: "Firmware", value: firmwareSummary)
        DeviceFactRow(systemName: "battery.25percent", label: "Battery", value: batterySummary)
        DeviceFactRow(systemName: ble.batteryIsCharging == true ? "bolt.fill" : "powerplug", label: "Charging", value: ble.batteryChargeDisplayStatus)
        DeviceFactRow(systemName: "bolt.batteryblock", label: "Battery pack", value: ble.batteryPackDisplaySummary)
        DeviceFactRow(systemName: "arrow.2.circlepath", label: "Last sync", value: relativeSummary(for: ble.lastSyncAt) ?? "Not synced")
        DeviceFactRow(systemName: "clock.arrow.circlepath", label: "Strap clock", value: clockSummary)
      }

      DeviceFactRow(systemName: "iphone", label: "Model", value: modelSummary)

      DeviceDetailStack {
        DeviceFactRow(systemName: "heart", label: "Live HR", value: heartRateSummary)
        DeviceFactRow(systemName: "dot.radiowaves.left.and.right", label: "Connection", value: ble.connectionState.capitalized)
        DeviceFactRow(systemName: "arrow.triangle.2.circlepath", label: "Historical sync", value: ble.historicalSyncStatus.capitalized)
        if ble.isHistoricalSyncing {
          HistoricalSyncProgressBar(
            fraction: ble.historicalSyncProgressFraction,
            packetCount: ble.historicalPacketCount,
            packetsRemaining: ble.historicalSyncPacketsRemaining,
            etaSeconds: ble.historicalSyncEtaSeconds
          )
        }
        DeviceFactRow(systemName: "bolt.horizontal", label: "High freq", value: ble.highFrequencyHistorySyncDisplaySummary)
        DeviceFactRow(systemName: "lungs", label: "RR packets", value: model.respiratoryPacketWatchStatus)
        DeviceFactRow(systemName: "cpu", label: "Rust", value: model.rustStatus)
        DeviceFactRow(systemName: "waveform.path.ecg", label: "Last frame", value: packetMonitor.lastParsedFrameSummary)
      }

      DeviceActionGrid(model: model, ble: ble)
      ReconnectBackoffBanner(ble: ble)
      DiscoveredDeviceList(ble: ble)
      EventLogPreview(messages: Array(messageStore.messages.prefix(5)))
    }
    .onAppear(perform: refreshClockIfPossible)
    .onChange(of: ble.connectionState) { _, _ in
      refreshClockIfPossible()
    }
  }

  private var firmwareSummary: String {
    ble.firmwareVersion ?? ble.softwareRevision ?? "Unknown"
  }

  private var batterySummary: String {
    guard let battery = ble.batteryLevelPercent else {
      return "Unknown"
    }
    let status = ble.batteryPowerStatus == "Unknown" ? "" : " | \(ble.batteryPowerStatus)"
    if let updatedAt = ble.batteryUpdatedAt,
       Date().timeIntervalSince(updatedAt) > 3600,
       let relative = relativeSummary(for: updatedAt) {
      return "\(battery)%\(status) [\(relative)]"
    }
    return "\(battery)%\(status)"
  }

  private var modelSummary: String {
    if let modelNumber = ble.modelNumber {
      return modelNumber
    }
    if let hardwareRevision = ble.hardwareRevision {
      return "Hardware \(hardwareRevision)"
    }
    return ble.activeDeviceName
  }

  private var heartRateSummary: String {
    guard let bpm = ble.liveHeartRateBPM else {
      return ble.liveHeartRateSource.capitalized
    }
    if let updatedAt = ble.liveHeartRateUpdatedAt,
       let relative = relativeSummary(for: updatedAt) {
      return "\(bpm) bpm \(relative)"
    }
    return "\(bpm) bpm"
  }

  private var clockSummary: String {
    guard let offset = ble.strapClockOffsetSeconds else {
      return ble.strapClockStatus
    }
    let drift = formattedClockOffset(offset)
    if let updatedAt = ble.strapClockUpdatedAt,
       let relative = relativeSummary(for: updatedAt) {
      return "\(drift) | \(ble.strapClockStatus) | \(relative)"
    }
    return "\(drift) | \(ble.strapClockStatus)"
  }

  private func refreshClockIfPossible() {
    guard ble.canSyncClock else {
      return
    }
    ble.readStrapClock(syncIfNeeded: true)
  }

  private func formattedClockOffset(_ offset: TimeInterval) -> String {
    let rounded = Int(offset.rounded())
    if rounded == 0 {
      return "0s"
    }
    let sign = rounded > 0 ? "+" : "-"
    return "\(sign)\(abs(rounded))s"
  }
}

private struct DeviceDetailStack<Content: View>: View {
  let content: Content

  init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  var body: some View {
    VStack(spacing: 0) {
      content
    }
  }
}

private struct DeviceFactRow: View {
  let systemName: String
  let label: String
  let value: String

  var body: some View {
    HStack(spacing: 12) {
      Image(systemName: systemName)
        .font(.system(size: 20, weight: .semibold))
        .foregroundStyle(secondaryText)
        .frame(width: 24)
      Text(label)
        .font(advancedBodyFont)
        .foregroundStyle(secondaryText)
        .lineLimit(1)
      Spacer(minLength: 16)
      Text(value)
        .font(advancedBodyFont)
        .foregroundStyle(devicePrimaryText)
        .lineLimit(1)
        .minimumScaleFactor(0.72)
        .multilineTextAlignment(.trailing)
    }
    .padding(.vertical, 16)
    .overlay(alignment: .bottom) {
      Rectangle()
        .fill(dividerColor)
        .frame(height: 1)
    }
  }
}

private struct DeviceActionGrid: View {
  @ObservedObject var model: BullAppModel
  @ObservedObject var ble: BullBLEClient
  @State private var showEraseConfirm = false

  private let columns = [
    GridItem(.flexible(), spacing: 10),
    GridItem(.flexible(), spacing: 10),
  ]

  var body: some View {
    LazyVGrid(columns: columns, spacing: 10) {
      DeviceActionButton(title: "Bluetooth", systemName: "antenna.radiowaves.left.and.right") {
        ble.requestBluetooth()
      }
      DeviceActionButton(title: ble.isScanning ? "Stop Scan" : "Scan", systemName: "dot.radiowaves.left.and.right") {
        ble.isScanning ? ble.stopScan() : ble.startScan()
      }
      .disabled(!ble.canScan)

      DeviceActionButton(title: "Connect", systemName: "link") {
        ble.connectSelected()
      }
      .disabled(!ble.canConnect)

      DeviceActionButton(title: "Reconnect", systemName: "arrow.clockwise") {
        ble.resetReconnectBackoff()
        ble.reconnectRemembered()
      }
      .disabled(!ble.canReconnectRemembered)

      DeviceActionButton(title: ble.isHistoricalSyncing ? "Syncing" : "Sync", systemName: "arrow.triangle.2.circlepath") {
        ble.syncHistoricalPackets()
      }
      .disabled(!ble.canSyncHistorical)

      DeviceActionButton(title: ble.highFrequencyHistorySyncActive ? "Exit HF" : "High Freq", systemName: "bolt.horizontal") {
        if ble.highFrequencyHistorySyncActive {
          ble.exitHighFrequencyHistorySync()
        } else {
          ble.enterHighFrequencyHistorySync()
        }
      }
      .disabled(!ble.canWriteHighFrequencyHistorySync)

      DeviceActionButton(title: model.respiratoryPacketWatchActive ? "Stop RR" : "Watch RR", systemName: "lungs") {
        if model.respiratoryPacketWatchActive {
          model.stopRespiratoryPacketWatch()
        } else {
          model.startRespiratoryPacketWatch()
        }
      }
      .disabled(!model.respiratoryPacketWatchActive && ble.connectionState != "ready")

      DeviceActionButton(title: "Hello", systemName: "paperplane") {
        ble.sendClientHello()
      }
      .disabled(!ble.canSendHello)

      DeviceActionButton(title: "Clock", systemName: "clock.arrow.circlepath") {
        ble.readStrapClock(syncIfNeeded: true)
      }
      .disabled(!ble.canSyncClock)

      DeviceActionButton(title: "Forget", systemName: "trash", role: .destructive) {
        ble.forgetRememberedDevice()
      }
      .disabled(!ble.hasRememberedDevice)

      DeviceActionButton(title: "Clear Band", systemName: "trash.slash", role: .destructive) {
        showEraseConfirm = true
      }
      .disabled(ble.connectionState != "ready")
    }
    .confirmationDialog(
      "Clear all data stored on the band?",
      isPresented: $showEraseConfirm,
      titleVisibility: .visible
    ) {
      Button("Clear Band Data", role: .destructive) { ble.eraseBandData() }
      Button("Cancel", role: .cancel) {}
    } message: {
      Text("This erases the band's stored buffer. Any data not yet synced to this phone is lost permanently. Already-synced data is safe.")
    }
  }
}

private struct DeviceActionButton: View {
  let title: String
  let systemName: String
  var role: ButtonRole?
  let action: () -> Void

  var body: some View {
    Button(role: role, action: action) {
      HStack(spacing: 8) {
        Image(systemName: systemName)
          .font(.system(size: 15, weight: .bold))
        Text(title)
          .font(.system(size: 15, weight: .black, design: .default))
          .lineLimit(1)
          .minimumScaleFactor(0.78)
      }
      .frame(maxWidth: .infinity, minHeight: 46)
      .padding(.horizontal, 10)
      .foregroundStyle(role == .destructive ? disconnectedRed : devicePrimaryText)
      .background(controlBackground, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
    .buttonStyle(.plain)
    .opacity(isDisabled ? 0.45 : 1)
  }

  @Environment(\.isEnabled) private var isEnabled

  private var isDisabled: Bool {
    !isEnabled
  }
}

private struct DiscoveredDeviceList: View {
  @ObservedObject var ble: BullBLEClient

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("DISCOVERED")
        .font(deviceLabelFont)
        .foregroundStyle(secondaryText)
      if ble.discoveredDevices.isEmpty {
        Text("No devices yet")
          .font(deviceBodyFont)
          .foregroundStyle(mutedText)
          .frame(maxWidth: .infinity, alignment: .leading)
      } else {
        VStack(spacing: 0) {
          ForEach(ble.discoveredDevices) { device in
            Button {
              ble.select(device)
            } label: {
              HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                  Text(device.name)
                    .font(deviceBodyFont.weight(.black))
                    .foregroundStyle(devicePrimaryText)
                    .lineLimit(1)
                  Text(device.id.uuidString)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(mutedText)
                    .lineLimit(1)
                }
                Spacer()
                Text("\(device.rssi)")
                  .font(deviceBodyFont.weight(.black))
                  .foregroundStyle(secondaryText)
              }
              .padding(.vertical, 13)
              .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .overlay(alignment: .bottom) {
              Rectangle()
                .fill(dividerColor)
                .frame(height: 1)
            }
          }
        }
      }
    }
  }
}

private struct EventLogPreview: View {
  let messages: [BullMessage]

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("EVENTS")
        .font(deviceLabelFont)
        .foregroundStyle(secondaryText)
      if messages.isEmpty {
        Text("No events yet")
          .font(deviceBodyFont)
          .foregroundStyle(mutedText)
      } else {
        VStack(spacing: 0) {
          ForEach(messages) { message in
            VStack(alignment: .leading, spacing: 5) {
              HStack(spacing: 8) {
                Text(message.timestamp, style: .time)
                Text(message.level.rawValue.uppercased())
                Text(message.source)
              }
              .font(.system(size: 12, weight: .bold, design: .default))
              .foregroundStyle(mutedText)

              Text(message.title)
                .font(.system(size: 15, weight: .black, design: .default))
                .foregroundStyle(devicePrimaryText)
                .lineLimit(1)

              if !message.body.isEmpty {
                Text(message.body)
                  .font(.system(size: 12, weight: .semibold, design: .monospaced))
                  .foregroundStyle(secondaryText)
                  .lineLimit(2)
              }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, 12)
            .overlay(alignment: .bottom) {
              Rectangle()
                .fill(dividerColor)
                .frame(height: 1)
            }
          }
        }
      }
    }
  }
}

private struct ReconnectBackoffBanner: View {
  @ObservedObject var ble: BullBLEClient

  var body: some View {
    if ble.reconnectAttemptCount > 0 {
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 8) {
          Image(systemName: "antenna.radiowaves.left.and.right.slash")
            .font(.system(size: 16, weight: .bold))
            .foregroundStyle(reconnectAmber)
          Text(reconnectHeadline)
            .font(.system(size: 15, weight: .black, design: .default))
            .foregroundStyle(devicePrimaryText)
            .lineLimit(2)
            .minimumScaleFactor(0.78)
        }

        if let retryAt = ble.reconnectNextRetryAt {
          Text("Next retry \(retryAt, style: .relative)")
            .font(.system(size: 13, weight: .semibold, design: .default))
            .foregroundStyle(secondaryText)
        }

        HStack(spacing: 10) {
          Button {
            ble.resetReconnectBackoff()
            ble.reconnectRemembered()
          } label: {
            Text("Retry Now")
              .font(.system(size: 14, weight: .black, design: .default))
              .foregroundStyle(devicePrimaryText)
              .padding(.horizontal, 14)
              .padding(.vertical, 8)
              .background(controlBackground, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
          }
          .buttonStyle(.plain)

          Button {
            ble.resetReconnectBackoff()
          } label: {
            Text("Stop Retrying")
              .font(.system(size: 14, weight: .black, design: .default))
              .foregroundStyle(disconnectedRed)
              .padding(.horizontal, 14)
              .padding(.vertical, 8)
              .background(controlBackground, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
          }
          .buttonStyle(.plain)
        }
      }
      .padding(14)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(reconnectBannerBackground)
      )
    }
  }

  private var reconnectHeadline: String {
    if ble.reconnectAttemptCount > BullBLEClient.reconnectMaxAttempts {
      return "Searching for your band…"
    }
    return "Reconnecting \(ble.reconnectAttemptCount)/\(BullBLEClient.reconnectMaxAttempts)"
  }
}

private func relativeSummary(for date: Date?) -> String? {
  guard let date else {
    return nil
  }
  if abs(date.timeIntervalSinceNow) < 10 {
    return "Now"
  }
  let formatter = RelativeDateTimeFormatter()
  formatter.unitsStyle = .short
  return formatter.localizedString(for: date, relativeTo: Date()).capitalized
}

private let deviceScreenBackground = BullTheme.appBackground
private let devicePrimaryText = Color(uiColor: .label)
private let controlBackground = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.12, green: 0.16, blue: 0.18, alpha: 1)
    : .secondarySystemGroupedBackground
})
private let deviceRailBackground = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.23, green: 0.25, blue: 0.27, alpha: 1)
    : .systemGray4
})
private let dividerColor = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.19, green: 0.22, blue: 0.25, alpha: 1)
    : .separator
})
private let secondaryText = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.63, green: 0.65, blue: 0.67, alpha: 1)
    : .secondaryLabel
})
private let mutedText = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.56, green: 0.58, blue: 0.60, alpha: 1)
    : .tertiaryLabel
})
private let connectedGreen = Color(red: 0.42, green: 0.84, blue: 0.30)
private let disconnectedRed = Color(red: 1.0, green: 0.27, blue: 0.23)
private let batteryYellow = Color(red: 1.0, green: 0.89, blue: 0.36)
private let deviceLabelFont = Font.system(size: 15, weight: .black, design: .default)
private let deviceBodyFont = Font.system(size: 17, weight: .bold, design: .default)
private let advancedBodyFont = Font.system(size: 17, weight: .regular, design: .default)
private let reconnectAmber = Color(red: 1.0, green: 0.72, blue: 0.18)
private let reconnectBannerBackground = Color(uiColor: UIColor { traits in
  traits.userInterfaceStyle == .dark
    ? UIColor(red: 0.14, green: 0.12, blue: 0.08, alpha: 1)
    : UIColor(red: 1.0, green: 0.96, blue: 0.88, alpha: 1)
})

// MARK: - Historical sync progress

/// Real-progress bar for an active historical sync. Determinate once a
/// packets-per-page ratio has been learned (shows %); otherwise a spinner with
/// the always-real live packet count.
private struct HistoricalSyncProgressBar: View {
  let fraction: Double?
  let packetCount: Int
  let packetsRemaining: Int?
  let etaSeconds: Double?

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      if let fraction {
        ProgressView(value: min(max(fraction, 0), 1))
          .tint(.accentColor)
        Text(caption(percent: Int((min(max(fraction, 0), 1)) * 100)))
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        HStack(spacing: 8) {
          ProgressView()
          Text("\(packetCount.formatted()) packets synced…")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.vertical, 4)
  }

  private func caption(percent: Int) -> String {
    var parts = ["\(percent)%", "\(packetCount.formatted()) synced"]
    if let remaining = packetsRemaining, remaining > 0 {
      parts.append("~\(remaining.formatted()) left")
    }
    if let eta = etaSeconds, eta.isFinite, eta > 0 {
      parts.append("~\(Self.etaText(eta)) left")
    }
    return parts.joined(separator: " · ")
  }

  private static func etaText(_ seconds: Double) -> String {
    let total = Int(seconds.rounded())
    if total >= 3600 {
      return "\(total / 3600)h \((total % 3600) / 60)m"
    } else if total >= 60 {
      return "\(total / 60)m \(total % 60)s"
    }
    return "\(total)s"
  }
}
