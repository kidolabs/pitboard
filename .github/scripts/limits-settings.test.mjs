import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../../apps/macos/Sources/PitboardApp/Settings/LimitsSettings.swift', import.meta.url), 'utf8');

test('Limits no longer renders or changes warm-up controls', () => {
  for (const removed of ['model.machine.warm', 'settings.warm', '.setWarm(', '.setWarmRange(', 'timeOfDay(']) {
    assert.ok(!source.includes(removed), `Warm-up UI still references ${removed}`);
  }
});

test('Removing warm-up leaves account-switch and quota threshold controls intact', () => {
  for (const retained of ['settings.autoSwitch', 'settings.autoSwitchAt', '.setAutoSwitch(', '.formStyle(.grouped)']) {
    assert.ok(source.includes(retained), `Missing quota-switch control ${retained}`);
  }
});
