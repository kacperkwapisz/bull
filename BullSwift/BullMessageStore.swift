import Foundation

final class BullMessageStore: ObservableObject {
  @Published private(set) var messages: [BullMessage] = []

  private let maximumMessages: Int
  private let flushInterval: TimeInterval
  private var pendingMessages: [BullMessage] = []
  private var flushWorkItem: DispatchWorkItem?

  init(maximumMessages: Int, flushInterval: TimeInterval) {
    self.maximumMessages = maximumMessages
    self.flushInterval = flushInterval
  }

  func enqueue(_ message: BullMessage) {
    guard Thread.isMainThread else {
      DispatchQueue.main.async { [weak self] in
        self?.enqueue(message)
      }
      return
    }

    pendingMessages.append(message)
    guard flushWorkItem == nil else {
      return
    }

    let workItem = DispatchWorkItem { [weak self] in
      self?.flush()
    }
    flushWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + flushInterval, execute: workItem)
  }

  func flush() {
    guard Thread.isMainThread else {
      DispatchQueue.main.async { [weak self] in
        self?.flush()
      }
      return
    }

    flushWorkItem?.cancel()
    flushWorkItem = nil
    guard !pendingMessages.isEmpty else {
      return
    }

    messages.insert(contentsOf: pendingMessages.reversed(), at: 0)
    pendingMessages.removeAll(keepingCapacity: true)
    if messages.count > maximumMessages {
      messages.removeLast(messages.count - maximumMessages)
    }
  }
}
