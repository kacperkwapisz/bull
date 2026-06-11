import Foundation
import HealthKit

struct HealthKitProfileAutofill {
  let weightGrams: Int?
  let sourceSummary: String

  static let empty = HealthKitProfileAutofill(
    weightGrams: nil,
    sourceSummary: "No weight samples found"
  )

  var hasValues: Bool {
    weightGrams != nil
  }
}

struct HealthKitProfileImportResult {
  let status: String
  let autofill: HealthKitProfileAutofill
}

enum HealthKitProfileImporter {
  static var readTypes: Set<HKObjectType> {
    var types = Set<HKObjectType>()
    if let bodyMassType = HKObjectType.quantityType(forIdentifier: .bodyMass) {
      types.insert(bodyMassType)
    }
    return types
  }

  static func requestProfileAccess() async -> HealthKitProfileImportResult {
    await requestAccess()
  }

  private static func requestAccess() async -> HealthKitProfileImportResult {
    guard HKHealthStore.isHealthDataAvailable() else {
      return HealthKitProfileImportResult(status: "Unavailable on this device", autofill: .empty)
    }

    let store = HKHealthStore()
    do {
      try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
        store.requestAuthorization(toShare: Set<HKSampleType>(), read: Self.readTypes) { success, error in
          if let error {
            continuation.resume(throwing: error)
          } else if success {
            continuation.resume()
          } else {
            continuation.resume(throwing: HealthKitProfileImporterError.authorizationDenied)
          }
        }
      }
      let autofill = await latestMeasurements(store: store)
      return HealthKitProfileImportResult(
        status: autofill.hasValues ? "Requested in Health; \(autofill.sourceSummary)" : "Requested in Health",
        autofill: autofill
      )
    } catch {
      return HealthKitProfileImportResult(status: "Failed: \(error.localizedDescription)", autofill: .empty)
    }
  }

  static func latestMeasurements(store: HKHealthStore = HKHealthStore()) async -> HealthKitProfileAutofill {
    guard HKHealthStore.isHealthDataAvailable() else {
      return .empty
    }

    async let weightSample = latestQuantitySample(
      store: store,
      type: HKObjectType.quantityType(forIdentifier: .bodyMass)
    )

    let latestWeightSample = await weightSample

    let weight = latestWeightSample.flatMap { sample -> Int? in
      let kilograms = sample.quantity.doubleValue(for: HKUnit.gramUnit(with: .kilo))
      guard kilograms > 0 else {
        return nil
      }
      return Int((kilograms * 1000).rounded())
    }

    var filled: [String] = []
    if weight != nil {
      filled.append("weight")
    }

    return HealthKitProfileAutofill(
      weightGrams: weight,
      sourceSummary: filled.isEmpty ? "No weight samples found" : "Filled \(filled.joined(separator: " and ")) from Apple Health"
    )
  }

  private static func latestQuantitySample(store: HKHealthStore, type: HKQuantityType?) async -> HKQuantitySample? {
    guard let type else {
      return nil
    }
    return await withCheckedContinuation { continuation in
      let sort = NSSortDescriptor(key: HKSampleSortIdentifierEndDate, ascending: false)
      let query = HKSampleQuery(sampleType: type, predicate: nil, limit: 1, sortDescriptors: [sort]) { _, samples, _ in
        continuation.resume(returning: samples?.first as? HKQuantitySample)
      }
      store.execute(query)
    }
  }
}

enum HealthKitProfileImporterError: LocalizedError {
  case authorizationDenied

  var errorDescription: String? {
    "Health access was not allowed."
  }
}
