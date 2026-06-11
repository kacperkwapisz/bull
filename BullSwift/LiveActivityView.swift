import CoreLocation
import MapKit
import SwiftUI
import UIKit

struct LiveActivityView: View {
  @EnvironmentObject private var model: BullAppModel

  var body: some View {
    LiveActivityContentView(
      ble: model.ble,
      session: model.activitySession,
      locationTracker: model.activityLocationTracker
    )
    .environmentObject(model)
  }
}

