import CoreLocation
import MapKit
import SwiftUI
import UIKit

enum ActivityEnvironment {
  case outdoor
  case indoor
  case pool
}

enum ActivityKind: String, CaseIterable, Identifiable {
  case run
  case indoorRun
  case walk
  case indoorWalk
  case hike
  case roadRide
  case mountainBike
  case soccer
  case strength
  case hiit
  case yoga
  case row
  case indoorRide
  case elliptical
  case stairStepper
  case pilates
  case barre
  case functionalTraining
  case poolSwim

  var id: String { rawValue }

  var title: String {
    switch self {
    case .run: "Run"
    case .indoorRun: "Indoor Run"
    case .walk: "Walk"
    case .indoorWalk: "Indoor Walk"
    case .hike: "Hike"
    case .roadRide: "Road Ride"
    case .mountainBike: "MTB"
    case .soccer: "Soccer"
    case .strength: "Strength"
    case .hiit: "HIIT"
    case .yoga: "Yoga"
    case .row: "Row"
    case .indoorRide: "Indoor Ride"
    case .elliptical: "Elliptical"
    case .stairStepper: "Stair Stepper"
    case .pilates: "Pilates"
    case .barre: "Barre"
    case .functionalTraining: "Functional Training"
    case .poolSwim: "Pool Swim"
    }
  }

  var subtitle: String {
    switch environment {
    case .outdoor: "GPS + HR"
    case .indoor: "HR zones"
    case .pool: "HR + laps"
    }
  }

  var systemImage: String {
    switch self {
    case .run: "figure.run"
    case .indoorRun: "figure.run.treadmill"
    case .walk: "figure.walk"
    case .indoorWalk: "figure.walk.motion"
    case .hike: "figure.hiking"
    case .roadRide: "bicycle"
    case .mountainBike: "mountain.2"
    case .soccer: "soccerball"
    case .strength: "dumbbell"
    case .hiit: "flame"
    case .yoga: "figure.yoga"
    case .row: "figure.rower"
    case .indoorRide: "figure.indoor.cycle"
    case .elliptical: "figure.elliptical"
    case .stairStepper: "figure.stairs"
    case .pilates: "figure.core.training"
    case .barre: "figure.flexibility"
    case .functionalTraining: "figure.cross.training"
    case .poolSwim: "figure.pool.swim"
    }
  }

  var tint: Color {
    switch self {
    case .run: .orange
    case .indoorRun: .red
    case .walk: .green
    case .indoorWalk: .mint
    case .hike: .brown
    case .roadRide: .blue
    case .mountainBike: .mint
    case .soccer: .teal
    case .strength: .red
    case .hiit: .pink
    case .yoga: .purple
    case .row: .cyan
    case .indoorRide: .indigo
    case .elliptical: .yellow
    case .stairStepper: .orange
    case .pilates: .purple
    case .barre: .pink
    case .functionalTraining: .gray
    case .poolSwim: .cyan
    }
  }

  var environment: ActivityEnvironment {
    switch self {
    case .run, .walk, .hike, .roadRide, .mountainBike, .soccer:
      .outdoor
    case .poolSwim:
      .pool
    case .indoorRun, .indoorWalk, .strength, .hiit, .yoga, .row, .indoorRide, .elliptical, .stairStepper, .pilates, .barre, .functionalTraining:
      .indoor
    }
  }

  var usesGPS: Bool {
    environment == .outdoor
  }

  var trainingFocus: String {
    switch self {
    case .run: "Pace, route, and time in aerobic zones"
    case .indoorRun: "Treadmill time, HR zones, and steady pacing"
    case .walk: "Steady movement, HR drift, and route"
    case .indoorWalk: "Incline or treadmill time and low-zone volume"
    case .hike: "Distance, elevation context, and low-zone time"
    case .roadRide: "Speed, route, and sustained zone work"
    case .mountainBike: "Route, surges, and high-intensity bursts"
    case .soccer: "Field coverage, repeated efforts, and HR recovery"
    case .strength: "Work blocks, recovery gaps, and strain"
    case .hiit: "Intervals, peaks, and recovery between rounds"
    case .yoga: "Duration, low-zone control, and calm time"
    case .row: "Sustained effort, HR stability, and cadence later"
    case .indoorRide: "Zone control and steady-state effort"
    case .elliptical: "Low-impact cardio, zone control, and duration"
    case .stairStepper: "Climbing effort, sustained HR, and leg load"
    case .pilates: "Core work, control, and low-zone strain"
    case .barre: "Muscular endurance, control, and steady effort"
    case .functionalTraining: "Mixed-modal work, peaks, and recovery"
    case .poolSwim: "Session time, HR response, and lap support later"
    }
  }
}

struct HeartRateZone: Identifiable {
  let id: Int
  let title: String
  let range: String
  let color: Color

  static let maxHeartRate = 190

  static let zones = [
    HeartRateZone(id: 1, title: "Zone 1", range: "<60%", color: .blue),
    HeartRateZone(id: 2, title: "Zone 2", range: "60-70%", color: .green),
    HeartRateZone(id: 3, title: "Zone 3", range: "70-80%", color: .yellow),
    HeartRateZone(id: 4, title: "Zone 4", range: "80-90%", color: .orange),
    HeartRateZone(id: 5, title: "Zone 5", range: "90%+", color: .red),
  ]

  static func zoneID(for bpm: Int) -> Int {
    let percentage = Double(bpm) / Double(maxHeartRate)
    if percentage < 0.60 {
      return 1
    }
    if percentage < 0.70 {
      return 2
    }
    if percentage < 0.80 {
      return 3
    }
    if percentage < 0.90 {
      return 4
    }
    return 5
  }

  static func zone(for id: Int) -> HeartRateZone {
    zones.first { $0.id == id } ?? zones[0]
  }
}

enum PaceZone: String {
  case easy
  case steady
  case tempo
  case fast
  case unknown

  var title: String {
    switch self {
    case .easy: "Easy"
    case .steady: "Steady"
    case .tempo: "Tempo"
    case .fast: "Fast"
    case .unknown: "GPS"
    }
  }

  var color: Color {
    switch self {
    case .easy: .blue
    case .steady: .green
    case .tempo: .orange
    case .fast: .red
    case .unknown: .gray
    }
  }

  static func zone(secondsPerKilometer: TimeInterval, activity: ActivityKind) -> PaceZone {
    if activity == .roadRide || activity == .mountainBike {
      switch secondsPerKilometer {
      case ..<120: .fast
      case ..<180: .tempo
      case ..<240: .steady
      default: .easy
      }
    } else if activity == .walk || activity == .hike {
      switch secondsPerKilometer {
      case ..<540: .fast
      case ..<720: .tempo
      case ..<900: .steady
      default: .easy
      }
    } else {
      switch secondsPerKilometer {
      case ..<270: .fast
      case ..<330: .tempo
      case ..<420: .steady
      default: .easy
      }
    }
  }
}

struct ActivityRouteSegment: Identifiable {
  let id: Int
  let start: CLLocationCoordinate2D
  let end: CLLocationCoordinate2D
  let zone: PaceZone

  var coordinates: [CLLocationCoordinate2D] {
    [start, end]
  }
}

