import { Injectable, signal } from '@angular/core';

import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class ClipboardService {
  readonly lastCopiedLabel = signal<string | null>(null);
  readonly clearAfterSecs = signal(0);
  readonly remainingSecs = signal(0);

  private tickTimer: ReturnType<typeof setInterval> | null = null;

  async copy(value: string, label: string): Promise<void> {
    const secs = await call<number>('copy_to_clipboard', { value });
    this.clearAfterSecs.set(secs);
    this.lastCopiedLabel.set(label);
    this.remainingSecs.set(secs);
    if (this.tickTimer !== null) clearInterval(this.tickTimer);
    this.tickTimer = setInterval(() => {
      const remaining = this.remainingSecs() - 1;
      this.remainingSecs.set(Math.max(remaining, 0));
      if (remaining <= 0) {
        this.lastCopiedLabel.set(null);
        if (this.tickTimer !== null) {
          clearInterval(this.tickTimer);
          this.tickTimer = null;
        }
      }
    }, 1000);
  }
}
