import { describe, expect, it } from 'vitest';

import { SettingsBounds } from '../models/settings.model';
import { validateSettingsForm } from './settings.service';

const bounds: SettingsBounds = {
  autoLockSecs: { min: 30, max: 86_400, default: 300 },
  clipboardClearSecs: { min: 1, max: 600, default: 15 },
  passwordHistoryLimit: { min: 0, max: 50, default: 10 },
};

const form = (over: Partial<Record<keyof SettingsBounds, number | string>> = {}) => ({
  autoLockSecs: 300,
  clipboardClearSecs: 15,
  passwordHistoryLimit: 10,
  ...over,
});

describe('validateSettingsForm', () => {
  it('accepts values inside the published bounds', () => {
    const result = validateSettingsForm(form(), bounds);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.values).toEqual({
        autoLockSecs: 300,
        clipboardClearSecs: 15,
        passwordHistoryLimit: 10,
      });
    }
  });

  it('accepts both edges of every range', () => {
    for (const key of Object.keys(bounds) as (keyof SettingsBounds)[]) {
      for (const edge of [bounds[key].min, bounds[key].max]) {
        expect(validateSettingsForm(form({ [key]: edge }), bounds).ok).toBe(true);
      }
    }
  });

  it('rejects a value just outside either edge', () => {
    for (const key of Object.keys(bounds) as (keyof SettingsBounds)[]) {
      expect(validateSettingsForm(form({ [key]: bounds[key].min - 1 }), bounds).ok).toBe(false);
      expect(validateSettingsForm(form({ [key]: bounds[key].max + 1 }), bounds).ok).toBe(false);
    }
  });

  it('follows the bounds it is given rather than any hardcoded number', () => {
    // The whole point of publishing bounds: change them in Rust and the form
    // moves with them.
    const narrowed: SettingsBounds = {
      ...bounds,
      autoLockSecs: { min: 60, max: 120, default: 60 },
    };
    expect(validateSettingsForm(form({ autoLockSecs: 300 }), narrowed).ok).toBe(false);
    expect(validateSettingsForm(form({ autoLockSecs: 90 }), narrowed).ok).toBe(true);
  });

  it('rejects non-numeric input', () => {
    const result = validateSettingsForm(form({ autoLockSecs: '' }), bounds);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('Auto-lock');
  });

  it('truncates fractional input to whole seconds', () => {
    const result = validateSettingsForm(form({ autoLockSecs: 300.9 }), bounds);
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.values.autoLockSecs).toBe(300);
  });

  it('names the offending field in the message', () => {
    const result = validateSettingsForm(form({ clipboardClearSecs: 9999 }), bounds);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toBe('Clipboard clear delay must be between 1 and 600 seconds.');
    }
  });
});
