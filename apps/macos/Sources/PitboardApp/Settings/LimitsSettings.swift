import PitboardKit
import SwiftUI

struct LimitsSettings: View {
    let model: AppModel
    @ScaledMetric(relativeTo: .body) private var shareWidth: CGFloat = 56

    nonisolated static func share(_ typed: Int, of atLimit: AtLimitShown) -> UInt8 {
        UInt8(min(max(typed, Int(atLimit.lowest)), Int(atLimit.highest)))
    }

    var body: some View {
        let atLimit = model.machine.atLimit
        let share = Binding(
            get: { Int(atLimit.at) },
            set: {
                model.send(.setAtLimit(choice: atLimit.choice, at: Self.share($0, of: atLimit)))
            })
        Form {
            Section {
                Picker(
                    atLimit.heading,
                    selection: Binding(
                        get: { atLimit.choice },
                        set: { model.send(.setAtLimit(choice: $0, at: atLimit.at)) })
                ) {
                    ForEach(atLimit.choices, id: \.choice) { choice in
                        Text(choice.title).tag(choice.choice)
                    }
                }
                .accessibilityIdentifier("settings.atLimit")
                if atLimit.choice != .nothing {
                    LabeledContent(atLimit.shareLabel) {
                        HStack {
                            // The HIG pairs a stepper with a field where large changes are likely.
                            ShareField(atLimit: atLimit, width: shareWidth) {
                                model.send(.setAtLimit(choice: atLimit.choice, at: $0))
                            }
                            Stepper(
                                atLimit.shareLabel, value: share,
                                in: Int(atLimit.lowest)...Int(atLimit.highest)
                            )
                            .labelsHidden()
                            .accessibilityIdentifier("settings.atLimitShare")
                        }
                    }
                    .disabled(!atLimit.shareEnabled)
                }
                if let standing = atLimit.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.autoSwitchStanding")
                }
                if let page = URL(string: atLimit.terms.link) {
                    Link(atLimit.terms.linkTitle, destination: page)
                        .accessibilityIdentifier("settings.terms")
                }
            } footer: {
                if let note = atLimit.note { Text(note).footnote() }
            }
            .disabled(!atLimit.enabled)
        }
        .formStyle(.grouped)
    }
}

// Sent on Return or as focus leaves: a share clamped at each key left "120" in the field over 99.
private struct ShareField: View {
    let atLimit: AtLimitShown
    let width: CGFloat
    let send: (UInt8) -> Void
    @State private var typed = 0
    @FocusState private var editing: Bool

    var body: some View {
        TextField(atLimit.shareLabel, value: $typed, format: .percent)
            .labelsHidden()
            .textFieldStyle(.roundedBorder)
            .multilineTextAlignment(.trailing)
            .frame(width: width)
            .focused($editing)
            .onSubmit(take)
            .onChange(of: editing) {
                if !editing { take() }
            }
            .onChange(of: atLimit.at, initial: true) { typed = Int(atLimit.at) }
            .accessibilityIdentifier("settings.atLimitShareField")
    }

    private func take() {
        let share = LimitsSettings.share(typed, of: atLimit)
        typed = Int(share)
        if share != atLimit.at { send(share) }
    }
}
