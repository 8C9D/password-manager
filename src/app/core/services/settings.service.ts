import { Injectable, signal } from '@angular/core';

import { AppSettings } from '../models/settings.model';
import { call } from './tauri-invoke';

const DEFAULT_AUTO_LOCK_SECS = 300;

@Injectable({ providedIn: 'root' })
export class SettingsService {
  readonly settings = signal<AppSettings>({ autoLockSecs: DEFAULT_AUTO_LOCK_SECS });

  async load(): Promise<AppSettings> {
    const s = await call<AppSettings>('get_settings');
    this.settings.set(s);
    return s;
  }

  async update(autoLockSecs: number): Promise<AppSettings> {
    const s = await call<AppSettings>('update_settings', {
      input: { autoLockSecs },
    });
    this.settings.set(s);
    return s;
  }
}
