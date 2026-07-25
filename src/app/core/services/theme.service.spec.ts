import { describe, expect, it } from 'vitest';

import { parseThemePreference, THEME_OPTIONS } from './theme.service';

describe('parseThemePreference', () => {
  it('accepts the three known preferences', () => {
    expect(parseThemePreference('system')).toBe('system');
    expect(parseThemePreference('light')).toBe('light');
    expect(parseThemePreference('dark')).toBe('dark');
  });

  it('falls back to system for anything else', () => {
    // The value comes out of localStorage, which anything on the machine can
    // have written; an unknown value must not leave the app themeless.
    expect(parseThemePreference(null)).toBe('system');
    expect(parseThemePreference('')).toBe('system');
    expect(parseThemePreference('DARK')).toBe('system');
    expect(parseThemePreference('{"x":1}')).toBe('system');
  });
});

describe('THEME_OPTIONS', () => {
  it('offers exactly the preferences the parser accepts', () => {
    expect(THEME_OPTIONS.map((o) => o.value)).toEqual(['system', 'light', 'dark']);
    for (const option of THEME_OPTIONS) {
      expect(parseThemePreference(option.value)).toBe(option.value);
      expect(option.label).not.toBe('');
    }
  });
});
