import PitboardKit
import SwiftUI

struct LimitsSettings: View {
    let model: AppModel

    var body: some View {
        let auto = model.machine.autoSwitch
        let codex = model.machine.codexAutoSwitch
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
            Section {
                Toggle(
                    "Switch Codex/Pi automatically",
                    isOn: Binding(
                        get: { codex.on },
                        set: { model.send(.setCodexAutoSwitch(on: $0, at: codex.at)) })
                )
                .accessibilityIdentifier("settings.codexAutoSwitch")
                .disabled(!codex.enabled)
                Stepper(
                    codex.atLabel,
                    value: Binding(
                        get: { Int(codex.at) },
                        set: { model.send(.setCodexAutoSwitch(on: codex.on, at: UInt8($0))) }),
                    in: Int(codex.lowest)...Int(codex.highest)
                )
                .accessibilityIdentifier("settings.codexAutoSwitchAt")
                .disabled(!codex.enabled || !codex.on)
                if let standing = codex.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.codexAutoSwitchStanding")
                }
            } header: {
                Text("Codex/Pi — independent quota threshold")
            } footer: {
                Text(codex.note).footnote()
            }
        }
        .formStyle(.grouped)
    }
}
