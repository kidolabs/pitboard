import PitboardKit
import SwiftUI

struct LimitsSettings: View {
    let model: AppModel

    var body: some View {
        let auto = model.machine.autoSwitch
        Form {
            Section {
                Toggle(
                    "Switch Claude Code automatically",
                    isOn: Binding(
                        get: { auto.on },
                        set: { model.send(.setAutoSwitch(on: $0, at: auto.at)) })
                )
                .accessibilityIdentifier("settings.autoSwitch")
                .disabled(!auto.enabled)
                Stepper(
                    auto.atLabel,
                    value: Binding(
                        get: { Int(auto.at) },
                        set: { model.send(.setAutoSwitch(on: auto.on, at: UInt8($0))) }),
                    in: Int(auto.lowest)...Int(auto.highest)
                )
                .accessibilityIdentifier("settings.autoSwitchAt")
                .disabled(!auto.enabled || !auto.on)
                if let standing = auto.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.autoSwitchStanding")
                }
            } header: {
                Text("Before an account runs out")
            } footer: {
                Text(auto.note).footnote()
            }
        }
        .formStyle(.grouped)
    }
}
