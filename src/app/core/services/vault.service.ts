import { Injectable, signal } from '@angular/core';

import { VaultStatus } from '../models/entry.model';
import { call } from './tauri-invoke';

@Injectable({ providedIn: 'root' })
export class VaultService {
  readonly status = signal<VaultStatus | null>(null);
  readonly isUnlocked = signal(false);

  async refreshStatus(): Promise<VaultStatus> {
    const status = await call<VaultStatus>('vault_status');
    this.status.set(status);
    this.isUnlocked.set(status.unlocked);
    return status;
  }

  async createVault(masterPassword: string, vaultName?: string): Promise<void> {
    await call<void>('create_vault', {
      masterPassword,
      vaultName: vaultName ?? null,
    });
    this.isUnlocked.set(true);
    await this.refreshStatus();
  }

  async unlock(masterPassword: string): Promise<void> {
    await call<void>('unlock_vault', { masterPassword });
    this.isUnlocked.set(true);
    await this.refreshStatus();
  }

  async lock(): Promise<void> {
    await call<void>('lock_vault');
    this.isUnlocked.set(false);
    await this.refreshStatus();
  }
}
