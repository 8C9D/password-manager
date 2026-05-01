import { effect, inject, Injectable } from '@angular/core';
import { Router } from '@angular/router';

import { SettingsService } from './settings.service';
import { VaultService } from './vault.service';

const ACTIVITY_EVENTS = ['mousemove', 'keydown', 'mousedown', 'wheel', 'touchstart'] as const;

@Injectable({ providedIn: 'root' })
export class AutoLockService {
  private readonly vault = inject(VaultService);
  private readonly settings = inject(SettingsService);
  private readonly router = inject(Router);

  private timer: ReturnType<typeof setTimeout> | null = null;
  private settingsLoaded = false;

  private readonly onActivity = () => {
    if (this.vault.isUnlocked()) this.reset();
  };

  constructor() {
    for (const ev of ACTIVITY_EVENTS) {
      window.addEventListener(ev, this.onActivity, { passive: true });
    }
    window.addEventListener('visibilitychange', this.onActivity, { passive: true });

    effect(() => {
      const unlocked = this.vault.isUnlocked();
      const secs = this.settings.settings().autoLockSecs;

      if (unlocked && !this.settingsLoaded) {
        this.settingsLoaded = true;
        void this.settings.load().catch(() => {
          this.settingsLoaded = false;
        });
      }

      if (unlocked && secs > 0) {
        this.scheduleTimer(secs);
      } else {
        this.clearTimer();
      }
    });
  }

  reset(): void {
    if (!this.vault.isUnlocked()) {
      this.clearTimer();
      return;
    }
    const secs = this.settings.settings().autoLockSecs;
    if (secs > 0) this.scheduleTimer(secs);
  }

  private scheduleTimer(secs: number): void {
    this.clearTimer();
    this.timer = setTimeout(() => {
      void this.fire();
    }, secs * 1000);
  }

  private clearTimer(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private async fire(): Promise<void> {
    if (!this.vault.isUnlocked()) return;
    try {
      await this.vault.lock();
    } finally {
      await this.router.navigate(['/unlock']);
    }
  }
}
