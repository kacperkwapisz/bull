import SwiftUI
import UIKit

enum BullTheme {
  static let deviceBackground = Color(red: 0.06, green: 0.09, blue: 0.11)

  static let appBackground = Color(uiColor: UIColor { traits in
    traits.userInterfaceStyle == .dark ? deviceBackgroundUIColor : .systemGroupedBackground
  })

  static let plainBackground = Color(uiColor: UIColor { traits in
    traits.userInterfaceStyle == .dark ? deviceBackgroundUIColor : .systemBackground
  })

  static func configureAppearance() {
    UIWindow.appearance().backgroundColor = appBackgroundUIColor
    UITableView.appearance().backgroundColor = appBackgroundUIColor
    UICollectionView.appearance().backgroundColor = appBackgroundUIColor

    let navigationAppearance = UINavigationBarAppearance()
    navigationAppearance.configureWithTransparentBackground()
    navigationAppearance.backgroundEffect = UIBlurEffect(style: .systemChromeMaterial)
    navigationAppearance.backgroundColor = navigationBarBackgroundUIColor
    navigationAppearance.shadowColor = .clear
    UINavigationBar.appearance().standardAppearance = navigationAppearance
    UINavigationBar.appearance().compactAppearance = navigationAppearance
    UINavigationBar.appearance().scrollEdgeAppearance = navigationAppearance

    let tabAppearance = UITabBarAppearance()
    tabAppearance.configureWithOpaqueBackground()
    tabAppearance.backgroundColor = appBackgroundUIColor
    tabAppearance.shadowColor = .clear
    UITabBar.appearance().standardAppearance = tabAppearance
    UITabBar.appearance().scrollEdgeAppearance = tabAppearance
  }

  private static let deviceBackgroundUIColor = UIColor(
    red: 0.06,
    green: 0.09,
    blue: 0.11,
    alpha: 1
  )

  private static let appBackgroundUIColor = UIColor { traits in
    traits.userInterfaceStyle == .dark ? deviceBackgroundUIColor : .systemGroupedBackground
  }

  private static let navigationBarBackgroundUIColor = UIColor { traits in
    let alpha: CGFloat = traits.userInterfaceStyle == .dark ? 0.58 : 0.46
    return appBackgroundUIColor.resolvedColor(with: traits).withAlphaComponent(alpha)
  }
}

extension View {
  func bullScreenBackground() -> some View {
    background(BullTheme.appBackground.ignoresSafeArea())
  }

  func bullPlainBackground() -> some View {
    background(BullTheme.plainBackground.ignoresSafeArea())
  }

  func bullListBackground() -> some View {
    scrollContentBackground(.hidden)
      .background(BullTheme.appBackground.ignoresSafeArea())
  }
}
