import { Injectable, signal } from '@angular/core';

import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class ClipboardService {
  readonly lastCopiedLabel = signal<string | null>(null);
  readonly clearAfterSecs = signal(0);
  readonly remainingSecs = signal(0);

  private tickTimer: ReturnType<typeof setInterval> | null = null;
  // Bumped per copy so a slower earlier copy's response can't relabel the
  // banner after a newer copy already updated it.
  private copySeq = 0;

  // Stop the countdown and forget what was copied. Called when the vault
  // locks: the backend wipes the OS clipboard at that moment, so a surviving
  // banner would both lie about the clipboard contents and leak that a
  // secret had been copied in the previous session.
  reset(): void {
    if (this.tickTimer !== null) {
      clearInterval(this.tickTimer);
      this.tickTimer = null;
    }
    this.lastCopiedLabel.set(null);
    this.remainingSecs.set(0);
    this.clearAfterSecs.set(0);
  }

  async copy(value: string, label: string): Promise<void> {
    const seq = ++this.copySeq;
    const secs = await call<number>('copy_to_clipboard', { value });
    if (seq !== this.copySeq) return;
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
