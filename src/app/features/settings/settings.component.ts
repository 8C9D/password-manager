import {
  Component,
  inject,
  OnDestroy,
  OnInit,
  signal,
  WritableSignal,
} from '@angular/core';
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
export class SettingsComponent implements OnInit, OnDestroy {
  private readonly settings = inject(SettingsService);
  private readonly vault = inject(VaultService);

  // One tracked timer per message signal: a re-save within the flash window
  // must restart its own timer, not have the message wiped early by the
  // previous save's timer; all are cancelled on destroy.
  private readonly flashTimers = new Map<
    WritableSignal<string | null>,
    ReturnType<typeof setTimeout>
  >();

  private flash(sig: WritableSignal<string | null>, text: string, ms: number): void {
    sig.set(text);
    const prev = this.flashTimers.get(sig);
    if (prev !== undefined) clearTimeout(prev);
    this.flashTimers.set(
      sig,
      setTimeout(() => {
        sig.set(null);
        this.flashTimers.delete(sig);
      }, ms),
    );
  }

  ngOnDestroy(): void {
    for (const timer of this.flashTimers.values()) clearTimeout(timer);
    this.flashTimers.clear();
  }

  protected readonly presets = PRESETS;
  protected autoLockSecs = 300;
  protected clipboardClearSecs = 15;
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
      this.clipboardClearSecs = s.clipboardClearSecs;
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
    const clearSecs = Math.floor(Number(this.clipboardClearSecs));
    if (!Number.isFinite(clearSecs) || clearSecs < 1 || clearSecs > 600) {
      this.errorMsg.set('Clipboard clear delay must be between 1 and 600 seconds.');
      this.busy.set(false);
      return;
    }
    try {
      await this.settings.update({
        autoLockSecs: secs,
        clipboardClearSecs: clearSecs,
      });
      this.flash(this.savedMsg, 'Settings saved.', 3000);
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
      this.flash(this.exportSavedMsg, 'Encrypted export saved.', 5000);
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

  protected readonly csvBusy = signal(false);
  protected readonly csvErrorMsg = signal<string | null>(null);
  protected readonly csvSavedMsg = signal<string | null>(null);

  protected async onImportCsv(): Promise<void> {
    if (this.csvBusy()) return;
    this.csvBusy.set(true);
    this.csvErrorMsg.set(null);
    this.csvSavedMsg.set(null);
    try {
      const path = await open({
        title: 'Import passwords from CSV',
        multiple: false,
        filters: [{ name: 'CSV', extensions: ['csv'] }],
      });
      if (!path) return;
      const summary = await this.vault.importCsv(path);
      const parts = [
        `Imported ${summary.imported} ${summary.imported === 1 ? 'entry' : 'entries'}`,
      ];
      if (summary.categoriesCreated > 0) {
        parts.push(
          `${summary.categoriesCreated} new ${summary.categoriesCreated === 1 ? 'category' : 'categories'}`,
        );
      }
      if (summary.skipped > 0) {
        parts.push(`${summary.skipped} skipped`);
      }
      this.csvSavedMsg.set(`${parts.join(', ')}.`);
    } catch (e) {
      this.csvErrorMsg.set(formatBackendError(e));
    } finally {
      this.csvBusy.set(false);
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
      this.flash(this.pwSavedMsg, 'Master password changed.', 3000);
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
