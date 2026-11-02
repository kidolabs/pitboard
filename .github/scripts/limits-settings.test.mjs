import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../../apps/macos/Sources/PitboardApp/Settings/LimitsSettings.swift', import.meta.url), 'utf8');

test('Limits no longer renders or changes warm-up controls', () => {
  for (const removed of ['model.machine.warm', 'settings.warm', '.setWarm(', '.setWarmRange(', 'timeOfDay(']) {
    assert.ok(!source.includes(removed), `Warm-up UI still references ${removed}`);
  }
});

test('Codex/Pi has independent native controls and provider-specific core bindings', async () => {
  for (const binding of ['model.machine.codexAutoSwitch', '.setCodexAutoSwitch(', 'settings.codexAutoSwitch', 'settings.codexAutoSwitchAt', 'Codex/Pi']) {
    assert.ok(source.includes(binding), `Missing Codex/Pi control ${binding}`);
  }
  const launch = await readFile(new URL('../../crates/pitboard-ffi/src/launch.rs', import.meta.url), 'utf8');
  for (const api of ['.auto_look_for(provider, threshold)', '.auto_switch_for(provider, threshold)']) {
    assert.ok(launch.includes(api), `Missing provider-specific core API ${api}`);
  }
  const presentation = await readFile(new URL('../../crates/pitboard-ffi/src/present/machine.rs', import.meta.url), 'utf8');
  for (const explanation of ['next request', 'restart', 'DRAFT']) {
    assert.ok(presentation.includes(explanation), `Missing adoption/safety explanation ${explanation}`);
  }
});

test('Removing warm-up leaves account-switch and quota threshold controls intact', () => {
  for (const retained of ['settings.autoSwitch', 'settings.autoSwitchAt', '.setAutoSwitch(', '.formStyle(.grouped)']) {
    assert.ok(source.includes(retained), `Missing quota-switch control ${retained}`);
  }
});
