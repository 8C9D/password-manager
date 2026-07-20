import { Injectable, signal } from '@angular/core';

import { AppSettings } from '../models/settings.model';
import { call } from './tauri-invoke';

const DEFAULT_AUTO_LOCK_SECS = 300;
const DEFAULT_CLIPBOARD_CLEAR_SECS = 15;

@Injectable({ providedIn: 'root' })
export class SettingsService {
  readonly settings = signal<AppSettings>({
    autoLockSecs: DEFAULT_AUTO_LOCK_SECS,
    clipboardClearSecs: DEFAULT_CLIPBOARD_CLEAR_SECS,
  });

  async load(): Promise<AppSettings> {
    const s = await call<AppSettings>('get_settings');
    this.settings.set(s);
    return s;
  }

  async update(input: AppSettings): Promise<AppSettings> {
    const s = await call<AppSettings>('update_settings', { input });
    this.settings.set(s);
    return s;
  }
}
