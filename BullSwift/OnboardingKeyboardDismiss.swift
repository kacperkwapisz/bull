import SwiftUI
import UIKit

struct OnboardingKeyboardDismissTapCatcher: UIViewRepresentable {
  let isEnabled: Bool
  let dismiss: () -> Void

  func makeCoordinator() -> Coordinator {
    Coordinator(isEnabled: isEnabled, dismiss: dismiss)
  }

  func makeUIView(context: Context) -> UIView {
    let view = UIView(frame: .zero)
    view.isUserInteractionEnabled = false
    return view
  }

  func updateUIView(_ uiView: UIView, context: Context) {
    context.coordinator.isEnabled = isEnabled
    context.coordinator.dismiss = dismiss
    DispatchQueue.main.async {
      context.coordinator.attach(to: uiView.window)
    }
  }

  static func dismantleUIView(_ uiView: UIView, coordinator: Coordinator) {
    coordinator.detach()
  }

  final class Coordinator: NSObject, UIGestureRecognizerDelegate {
    var isEnabled: Bool
    var dismiss: () -> Void
    private weak var window: UIWindow?
    private weak var recognizer: UITapGestureRecognizer?

    init(isEnabled: Bool, dismiss: @escaping () -> Void) {
      self.isEnabled = isEnabled
      self.dismiss = dismiss
    }

    func attach(to nextWindow: UIWindow?) {
      guard let nextWindow else {
        return
      }
      if window === nextWindow {
        return
      }
      detach()
      let tapRecognizer = UITapGestureRecognizer(target: self, action: #selector(handleTap))
      tapRecognizer.cancelsTouchesInView = false
      tapRecognizer.delegate = self
      nextWindow.addGestureRecognizer(tapRecognizer)
      window = nextWindow
      recognizer = tapRecognizer
    }

    func detach() {
      if let recognizer, let window {
        window.removeGestureRecognizer(recognizer)
      }
      recognizer = nil
      window = nil
    }

    @objc private func handleTap() {
      guard isEnabled else {
        return
      }
      dismiss()
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
      guard isEnabled, let view = touch.view else {
        return false
      }
      return !view.hasSuperview(of: UIControl.self)
    }
  }
}

extension UIView {
  func hasSuperview<T: UIView>(of type: T.Type) -> Bool {
    var candidate: UIView? = self
    while let current = candidate {
      if current is T {
        return true
      }
      candidate = current.superview
    }
    return false
  }
}
