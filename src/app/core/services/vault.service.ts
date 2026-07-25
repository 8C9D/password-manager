import { computed, inject, Injectable, signal } from '@angular/core';

import { VaultStatus } from '../models/entry.model';
import { CategoryService } from './category.service';
import { ClipboardService } from './clipboard.service';
import { ConfirmService } from './confirm.service';
import { PasswordEntryService } from './password-entry.service';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class VaultService {
  private readonly entries = inject(PasswordEntryService);
  private readonly categories = inject(CategoryService);
  private readonly clipboard = inject(ClipboardService);
  private readonly confirm = inject(ConfirmService);

  readonly status = signal<VaultStatus | null>(null);
  readonly isUnlocked = computed(() => this.status()?.unlocked ?? false);

  async refreshStatus(): Promise<VaultStatus> {
    const status = await call<VaultStatus>('vault_status');
    this.status.set(status);
    return status;
  }

  async createVault(masterPassword: string, vaultName?: string): Promise<void> {
    await call<void>('create_vault', {
      masterPassword,
      vaultName: vaultName ?? null,
    });
    await this.refreshStatus();
  }

  async unlock(masterPassword: string): Promise<void> {
    await call<void>('unlock_vault', { masterPassword });
    await this.refreshStatus();
  }

  async changeMasterPassword(
    currentPassword: string,
    newPassword: string,
  ): Promise<void> {
    await call<void>('change_master_password', { currentPassword, newPassword });
  }

  async lock(): Promise<void> {
    await call<void>('lock_vault');
    // Purge cached vault state so entry/category metadata doesn't linger in
    // memory (or flash on screen) after the vault is locked. The clipboard
    // banner and any open confirm dialog belong to the unlocked session too.
    this.entries.clear();
    this.categories.clear();
    this.clipboard.reset();
    this.confirm.dismiss();
    await this.refreshStatus();
  }

  async exportVault(masterPassword: string, path: string): Promise<void> {
    await call<void>('export_vault', { masterPassword, path });
  }

  async exportCsv(masterPassword: string, path: string): Promise<void> {
    await call<void>('export_csv', { masterPassword, path });
  }

  async importVault(path: string, password: string): Promise<ImportSummary> {
    const summary = await call<ImportSummary>('import_vault', { path, password });
    await this.refreshAfterImport();
    return summary;
  }

  async importCsv(path: string): Promise<CsvImportSummary> {
    const summary = await call<CsvImportSummary>('import_csv', { path });
    await this.refreshAfterImport();
    return summary;
  }

  // The entry list and sidebar live in the persistent vault layout and only
  // load once per unlock; without an explicit refresh, imported entries and
  // categories stay invisible until the next lock/unlock cycle.
  private async refreshAfterImport(): Promise<void> {
    await Promise.all([this.entries.list(), this.categories.list()]);
  }
}

/**
 * Whether the create-vault form is valid: the master password is at least 8
 * characters and the confirmation matches it exactly.
 */
export function canCreateVault(pw1: string, pw2: string): boolean {
  return pw1.length >= 8 && pw1 === pw2;
}

export interface ImportSummary {
  entriesImported: number;
  categoriesCreated: number;
}

export interface CsvImportSummary {
  imported: number;
  skipped: number;
  categoriesCreated: number;
}
