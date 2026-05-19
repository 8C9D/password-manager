import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';

import { formatBackendError } from '../../core/services/tauri-invoke';
import { VaultService } from '../../core/services/vault.service';

type Mode = 'unknown' | 'create' | 'unlock';

@Component({
  selector: 'app-vault-unlock',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './vault-unlock.component.html',
  styleUrl: './vault-unlock.component.css',
})
export class VaultUnlockComponent {
  private readonly vault = inject(VaultService);
  private readonly router = inject(Router);

  protected pw1 = '';
  protected pw2 = '';
  protected readonly mode = signal<Mode>('unknown');
  protected readonly busy = signal(false);
  protected readonly errorMsg = signal<string | null>(null);

  constructor() {
    void this.init();
  }

  private async init(): Promise<void> {
    try {
      const status = await this.vault.refreshStatus();
      if (status.unlocked) {
        await this.router.navigate(['/vault']);
        return;
      }
      this.mode.set(status.exists ? 'unlock' : 'create');
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    }
  }

  protected canCreate(): boolean {
    return this.pw1.length >= 8 && this.pw1 === this.pw2;
  }

  protected async onCreate(): Promise<void> {
    if (!this.canCreate() || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      await this.vault.createVault(this.pw1);
      this.pw1 = '';
      this.pw2 = '';
      await this.router.navigate(['/vault']);
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.busy.set(false);
    }
  }

  protected async onUnlock(): Promise<void> {
    if (!this.pw1 || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      await this.vault.unlock(this.pw1);
      this.pw1 = '';
      await this.router.navigate(['/vault']);
    } catch (e) {
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.busy.set(false);
    }
  }
}
