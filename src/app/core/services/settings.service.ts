import { Injectable, signal } from '@angular/core';

import { AppSettings, Bound, SettingsBounds } from '../models/settings.model';
import { call } from './tauri-invoke';

const DEFAULT_AUTO_LOCK_SECS = 300;
const DEFAULT_CLIPBOARD_CLEAR_SECS = 15;
const DEFAULT_PASSWORD_HISTORY_LIMIT = 10;

/**
 * Placeholders used only until `loadBounds()` returns. The backend publishes
 * the real numbers; these exist so the form has something to render on its
 * first frame, and are deliberately the widest plausible ranges so they can
 * never reject a value the backend would have accepted.
 */
const FALLBACK_BOUNDS: SettingsBounds = {
  autoLockSecs: { min: 30, max: 86_400, default: DEFAULT_AUTO_LOCK_SECS },
  clipboardClearSecs: { min: 1, max: 600, default: DEFAULT_CLIPBOARD_CLEAR_SECS },
  passwordHistoryLimit: { min: 0, max: 50, default: DEFAULT_PASSWORD_HISTORY_LIMIT },
};

@Injectable({ providedIn: 'root' })
export class SettingsService {
  readonly settings = signal<AppSettings>({
    autoLockSecs: DEFAULT_AUTO_LOCK_SECS,
    clipboardClearSecs: DEFAULT_CLIPBOARD_CLEAR_SECS,
    passwordHistoryLimit: DEFAULT_PASSWORD_HISTORY_LIMIT,
  });

  readonly bounds = signal<SettingsBounds>(FALLBACK_BOUNDS);

  async load(): Promise<AppSettings> {
    const s = await call<AppSettings>('get_settings');
    this.settings.set(s);
    return s;
  }

  async loadBounds(): Promise<SettingsBounds> {
    const b = await call<SettingsBounds>('get_settings_bounds');
    this.bounds.set(b);
    return b;
  }

  async update(input: AppSettings): Promise<AppSettings> {
    const s = await call<AppSettings>('update_settings', { input });
    this.settings.set(s);
    return s;
  }
}

/** Render a bound's range for help text, e.g. "30–86400". */
export function describeBound(bound: Bound): string {
  return `${bound.min}–${bound.max}`;
}

export interface SettingsFormValues {
  autoLockSecs: number | string;
  clipboardClearSecs: number | string;
  passwordHistoryLimit: number | string;
}

/**
 * Validate the settings form against the backend's own bounds, returning the
 * whole-number values to submit or the first problem found.
 *
 * The numbers live in Rust; this only reads them, so a bound changed there can
 * no longer leave the form accepting what the backend rejects.
 */
export function validateSettingsForm(
  values: SettingsFormValues,
  bounds: SettingsBounds,
): { ok: true; values: AppSettings } | { ok: false; error: string } {
  const fields: [keyof SettingsBounds, Bound, string, string][] = [
    ['autoLockSecs', bounds.autoLockSecs, 'Auto-lock', 'seconds'],
    ['clipboardClearSecs', bounds.clipboardClearSecs, 'Clipboard clear delay', 'seconds'],
    ['passwordHistoryLimit', bounds.passwordHistoryLimit, 'Password history', 'entries'],
  ];
  const parsed: Partial<AppSettings> = {};
  for (const [key, bound, label, unit] of fields) {
    const n = Math.floor(Number(values[key]));
    if (!Number.isFinite(n) || n < bound.min || n > bound.max) {
      return {
        ok: false,
        error: `${label} must be between ${bound.min} and ${bound.max} ${unit}.`,
      };
    }
    parsed[key] = n;
  }
  return { ok: true, values: parsed as AppSettings };
}
