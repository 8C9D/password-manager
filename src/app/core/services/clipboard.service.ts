import { Injectable, signal } from '@angular/core';

import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class ClipboardService {
  readonly lastCopiedLabel = signal<string | null>(null);
  readonly clearAfterSecs = signal(0);

  private hideTimer: ReturnType<typeof setTimeout> | null = null;

  async copy(value: string, label: string): Promise<void> {
    const secs = await call<number>('copy_to_clipboard', { value });
    this.clearAfterSecs.set(secs);
    this.lastCopiedLabel.set(label);
    if (this.hideTimer !== null) clearTimeout(this.hideTimer);
    this.hideTimer = setTimeout(() => {
      this.lastCopiedLabel.set(null);
      this.hideTimer = null;
    }, secs * 1000);
  }
}
