import { Injectable, signal } from '@angular/core';

export type ThemePreference = 'system' | 'light' | 'dark';

const STORAGE_KEY = 'pm-theme';

export const THEME_OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

/** Narrow an untrusted stored value to a preference, defaulting to system. */
export function parseThemePreference(raw: string | null): ThemePreference {
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system';
}

/**
 * Explicit light/dark override.
 *
 * Kept in localStorage rather than the vault's settings table because the
 * theme has to apply on the unlock screen, and writing a vault setting needs
 * an unlocked vault. It is a display preference, not vault data - nothing
 * about it is secret.
 */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  readonly preference = signal<ThemePreference>('system');

  constructor() {
    this.preference.set(parseThemePreference(this.read()));
    this.apply(this.preference());
  }

  set(preference: ThemePreference): void {
    this.preference.set(preference);
    this.apply(preference);
    try {
      if (preference === 'system') {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, preference);
      }
    } catch {
      // Storage can be unavailable (private mode, disabled). The theme still
      // applies for this session; only remembering it is lost.
    }
  }

  private read(): string | null {
    try {
      return localStorage.getItem(STORAGE_KEY);
    } catch {
      return null;
    }
  }

  /**
   * "system" removes the attribute entirely so the `prefers-color-scheme`
   * media query in the global stylesheet is what decides.
   */
  private apply(preference: ThemePreference): void {
    const root = document.documentElement;
    if (preference === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', preference);
    }
  }
}
