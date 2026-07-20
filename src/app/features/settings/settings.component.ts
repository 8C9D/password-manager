import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { open, save } from '@tauri-apps/plugin-dialog';

import { SettingsService } from '../../core/services/settings.service';
import { formatBackendError } from '../../core/services/tauri-invoke';
import { VaultService } from '../../core/services/vault.service';
import { PasswordStrengthMeterComponent } from '../password-strength/password-strength-meter.component';

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
  imports: [FormsModule, RouterLink, PasswordStrengthMeterComponent],
  templateUrl: './settings.component.html',
  styleUrl: './settings.component.css',
})
export class SettingsComponent implements OnInit {
  private readonly settings = inject(SettingsService);
  private readonly vault = inject(VaultService);

  protected readonly presets = PRESETS;
  protected autoLockSecs = 300;
  protected readonly loading = signal(true);
  protected readonly busy = signal(false);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly savedMsg = signal<string | null>(null);

  protected currentPw = '';
  protected newPw1 = '';
  protected newPw2 = '';
  protected readonly showCurrentPw = signal(false);
  protected readonly showNewPw = signal(false);
  protected readonly pwBusy = signal(false);
  protected readonly pwErrorMsg = signal<string | null>(null);
  protected readonly pwSavedMsg = signal<string | null>(null);

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

  protected exportPw = '';
  protected readonly exportBusy = signal(false);
  protected readonly exportErrorMsg = signal<string | null>(null);
  protected readonly exportSavedMsg = signal<string | null>(null);

  protected importPw = '';
  protected readonly importBusy = signal(false);
  protected readonly importErrorMsg = signal<string | null>(null);
  protected readonly importSavedMsg = signal<string | null>(null);

  protected async onExport(): Promise<void> {
    if (!this.exportPw || this.exportBusy()) return;
    this.exportBusy.set(true);
    this.exportErrorMsg.set(null);
    this.exportSavedMsg.set(null);
    try {
      const date = new Date().toISOString().slice(0, 10);
      const path = await save({
        title: 'Export vault',
        defaultPath: `vault-export-${date}.json`,
        filters: [{ name: 'Vault export', extensions: ['json'] }],
      });
      if (!path) return;
      await this.vault.exportVault(this.exportPw, path);
      this.exportPw = '';
      this.exportSavedMsg.set('Encrypted export saved.');
      setTimeout(() => this.exportSavedMsg.set(null), 5000);
    } catch (e) {
      this.exportErrorMsg.set(
        formatBackendError(e, {
          wrong_password: 'Master password is incorrect.',
        }),
      );
    } finally {
      this.exportBusy.set(false);
    }
  }

  protected async onImport(): Promise<void> {
    if (!this.importPw || this.importBusy()) return;
    this.importBusy.set(true);
    this.importErrorMsg.set(null);
    this.importSavedMsg.set(null);
    try {
      const path = await open({
        title: 'Import vault export',
        multiple: false,
        filters: [{ name: 'Vault export', extensions: ['json'] }],
      });
      if (!path) return;
      const summary = await this.vault.importVault(path, this.importPw);
      this.importPw = '';
      const cats =
        summary.categoriesCreated > 0
          ? ` and ${summary.categoriesCreated} new ${summary.categoriesCreated === 1 ? 'category' : 'categories'}`
          : '';
      this.importSavedMsg.set(
        `Imported ${summary.entriesImported} ${summary.entriesImported === 1 ? 'entry' : 'entries'}${cats}.`,
      );
    } catch (e) {
      this.importErrorMsg.set(
        formatBackendError(e, {
          wrong_password: 'That password does not match this export file.',
        }),
      );
    } finally {
      this.importBusy.set(false);
    }
  }

  protected canChangePassword(): boolean {
    return (
      this.currentPw.length > 0 &&
      this.newPw1.length >= 8 &&
      this.newPw1 === this.newPw2
    );
  }

  protected async onChangePassword(): Promise<void> {
    if (!this.canChangePassword() || this.pwBusy()) return;
    this.pwBusy.set(true);
    this.pwErrorMsg.set(null);
    this.pwSavedMsg.set(null);
    try {
      await this.vault.changeMasterPassword(this.currentPw, this.newPw1);
      this.currentPw = '';
      this.newPw1 = '';
      this.newPw2 = '';
      this.showCurrentPw.set(false);
      this.showNewPw.set(false);
      this.pwSavedMsg.set('Master password changed.');
      setTimeout(() => this.pwSavedMsg.set(null), 3000);
    } catch (e) {
      this.pwErrorMsg.set(
        formatBackendError(e, {
          wrong_password: 'Current password is incorrect.',
        }),
      );
    } finally {
      this.pwBusy.set(false);
    }
  }
}
