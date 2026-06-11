import SwiftUI

struct CoachComposer: View {
  @Binding var draft: String
  let focused: FocusState<Bool>.Binding
  let isStreaming: Bool
  let send: () -> Void
  let cancel: () -> Void
  @State private var inputHeight: CGFloat = 38

  private var trimmedDraft: String {
    draft.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var canSend: Bool {
    !trimmedDraft.isEmpty && !isStreaming
  }

  private var isMultilineInput: Bool {
    inputHeight > 42 || draft.contains("\n")
  }

  private var inputCornerRadius: CGFloat {
    isMultilineInput ? 14 : 19
  }

  var body: some View {
    VStack(spacing: 0) {
      HStack(alignment: .bottom, spacing: 8) {
        TextField("Ask Coach", text: $draft, axis: .vertical)
          .lineLimit(1...4)
          .submitLabel(.send)
          .textFieldStyle(.plain)
          .textInputAutocapitalization(.sentences)
          .focused(focused)
          .padding(.horizontal, 15)
          .padding(.vertical, 8)
          .frame(minHeight: 38)
          .background {
            GeometryReader { proxy in
              Color.clear
                .preference(key: CoachComposerInputHeightKey.self, value: proxy.size.height)
            }
          }
          .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: inputCornerRadius, style: .continuous))
          .overlay {
            RoundedRectangle(cornerRadius: inputCornerRadius, style: .continuous)
              .stroke(focused.wrappedValue ? Color.blue.opacity(0.40) : Color(.separator).opacity(0.18), lineWidth: 1)
          }
          .animation(.easeOut(duration: 0.16), value: isMultilineInput)
          .onPreferenceChange(CoachComposerInputHeightKey.self) { height in
            inputHeight = height
          }
          .onSubmit {
            if canSend {
              send()
            }
          }

        CoachComposerActionButton(
          isStreaming: isStreaming,
          canSend: canSend,
          action: isStreaming ? cancel : send
        )
      }
    }
    .padding(.horizontal, 12)
    .padding(.top, 7)
    .padding(.bottom, 7)
    .background(.regularMaterial)
    .overlay(alignment: .top) {
      Divider()
        .opacity(0.6)
    }
  }
}

private struct CoachComposerInputHeightKey: PreferenceKey {
  static var defaultValue: CGFloat = 38

  static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
    value = nextValue()
  }
}

private struct CoachComposerActionButton: View {
  let isStreaming: Bool
  let canSend: Bool
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      ZStack {
        Circle()
          .fill(background)
          .frame(width: 34, height: 34)
        Image(systemName: isStreaming ? "stop.fill" : "arrow.up")
          .font(.system(size: 16, weight: .bold))
          .foregroundStyle(foreground)
      }
      .frame(width: 38, height: 38)
    }
    .buttonStyle(.plain)
    .disabled(!isStreaming && !canSend)
    .accessibilityLabel(isStreaming ? "Stop streaming" : "Send message")
  }

  private var background: Color {
    if isStreaming {
      return .red
    }
    return canSend ? .blue : Color(.tertiarySystemFill)
  }

  private var foreground: Color {
    canSend || isStreaming ? .white : .secondary
  }
}
