import { Injectable, signal } from '@angular/core';

import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class ClipboardService {
  readonly lastCopiedLabel = signal<string | null>(null);
  readonly clearAfterSecs = 15;

  private hideTimer: ReturnType<typeof setTimeout> | null = null;

  async copy(value: string, label: string): Promise<void> {
    await call<void>('copy_to_clipboard', {
      value,
      clearAfterSecs: this.clearAfterSecs,
    });
    this.lastCopiedLabel.set(label);
    if (this.hideTimer !== null) clearTimeout(this.hideTimer);
    this.hideTimer = setTimeout(() => {
      this.lastCopiedLabel.set(null);
      this.hideTimer = null;
    }, this.clearAfterSecs * 1000);
  }
}
