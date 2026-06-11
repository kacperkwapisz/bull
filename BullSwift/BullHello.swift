import Foundation
import CryptoKit

enum BullHello {
  static let clientHelloFrameHex = "aa0108000001e67123019101363e5c8d"

  static var clientHelloFrame: Data {
    Data(hexString: clientHelloFrameHex) ?? Data()
  }
}

extension Data {
  init?(hexString: String) {
    let normalized = hexString.filter { !$0.isWhitespace }
    guard normalized.count.isMultiple(of: 2) else {
      return nil
    }

    var bytes: [UInt8] = []
    bytes.reserveCapacity(normalized.count / 2)

    var index = normalized.startIndex
    while index < normalized.endIndex {
      let next = normalized.index(index, offsetBy: 2)
      guard let byte = UInt8(normalized[index..<next], radix: 16) else {
        return nil
      }
      bytes.append(byte)
      index = next
    }
    self = Data(bytes)
  }

  var hexString: String {
    map { String(format: "%02x", $0) }.joined()
  }

  var sha256HexString: String {
    SHA256.hash(data: self).map { String(format: "%02x", $0) }.joined()
  }
}
