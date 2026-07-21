import { computed, Injectable, signal } from '@angular/core';

import { VaultStatus } from '../models/entry.model';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class VaultService {
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
    await this.refreshStatus();
  }

  async exportVault(masterPassword: string, path: string): Promise<void> {
    await call<void>('export_vault', { masterPassword, path });
  }

  async importVault(path: string, password: string): Promise<ImportSummary> {
    return call<ImportSummary>('import_vault', { path, password });
  }

  async importCsv(path: string): Promise<CsvImportSummary> {
    return call<CsvImportSummary>('import_csv', { path });
  }
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
