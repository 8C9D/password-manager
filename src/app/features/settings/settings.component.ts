import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';

import { SettingsService } from '../../core/services/settings.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

interface Preset {
  label: string;
  secs: number;
}

const PRESETS: Preset[] = [
  { label: '1 minute', secs: 60 },
  { label: '5 minutes', secs: 300 },
  { label: '15 minutes', secs: 900 },
  { label: '30 minutes', secs: 1800 },
  { label: '1 hour', secs: 3600 },
];

@Component({
  selector: 'app-settings',
  standalone: true,
  imports: [FormsModule, RouterLink],
  templateUrl: './settings.component.html',
  styleUrl: './settings.component.css',
})
export class SettingsComponent implements OnInit {
  private readonly settings = inject(SettingsService);

  protected readonly presets = PRESETS;
  protected autoLockSecs = 300;
  protected readonly loading = signal(true);
  protected readonly busy = signal(false);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly savedMsg = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    try {
      const s = await this.settings.load();
      this.autoLockSecs = s.autoLockSecs;
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected async onSave(): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    this.savedMsg.set(null);
    const secs = Math.floor(Number(this.autoLockSecs));
    if (!Number.isFinite(secs) || secs < 30 || secs > 86400) {
      this.errorMsg.set('Auto-lock must be between 30 seconds and 24 hours.');
      this.busy.set(false);
      return;
    }
    try {
      await this.settings.update(secs);
      this.savedMsg.set('Settings saved.');
      setTimeout(() => this.savedMsg.set(null), 3000);
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.busy.set(false);
    }
  }
}
