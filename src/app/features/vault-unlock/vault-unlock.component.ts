import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';

import { isBackendError } from '../../core/services/tauri-invoke';
import { VaultService } from '../../core/services/vault.service';

type Mode = 'unknown' | 'create' | 'unlock';

@Component({
  selector: 'app-vault-unlock',
  standalone: true,
  imports: [FormsModule],
  template: `
    <div class="unlock-shell">
      <div class="card">
        <h1>Password Manager</h1>
        @switch (mode()) {
          @case ('unknown') {
            <p class="muted">Checking vault…</p>
          }
          @case ('create') {
            <p>Set a master password to create your vault. You'll need it every time you unlock.</p>
            <form (ngSubmit)="onCreate()">
              <label>
                Master password
                <input
                  type="password"
                  [(ngModel)]="pw1"
                  name="pw1"
                  autocomplete="new-password"
                  minlength="8"
                  required
                />
              </label>
              <label>
                Confirm master password
                <input
                  type="password"
                  [(ngModel)]="pw2"
                  name="pw2"
                  autocomplete="new-password"
                  required
                />
              </label>
              @if (pw1.length > 0 && pw1.length < 8) {
                <p class="warn">Use at least 8 characters.</p>
              }
              @if (pw1 && pw2 && pw1 !== pw2) {
                <p class="warn">Passwords do not match.</p>
              }
              @if (errorMsg()) {
                <p class="warn">{{ errorMsg() }}</p>
              }
              <button
                type="submit"
                class="btn primary"
                [disabled]="!canCreate() || busy()"
              >
                {{ busy() ? 'Creating…' : 'Create vault' }}
              </button>
              <p class="muted small">
                The master password cannot be recovered. If you forget it, your data is unrecoverable.
              </p>
            </form>
          }
          @case ('unlock') {
            <p>Enter your master password to unlock the vault.</p>
            <form (ngSubmit)="onUnlock()">
              <label>
                Master password
                <input
                  type="password"
                  [(ngModel)]="pw1"
                  name="pw1"
                  autocomplete="current-password"
                  required
                />
              </label>
              @if (errorMsg()) {
                <p class="warn">{{ errorMsg() }}</p>
              }
              <button
                type="submit"
                class="btn primary"
                [disabled]="!pw1 || busy()"
              >
                {{ busy() ? 'Unlocking…' : 'Unlock' }}
              </button>
            </form>
          }
        }
      </div>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        height: 100%;
      }
      .unlock-shell {
        display: grid;
        place-items: center;
        min-height: 100vh;
        padding: 2rem;
        background: var(--bg);
      }
      .card {
        width: 100%;
        max-width: 420px;
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 2rem;
        box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
      }
      h1 {
        margin: 0 0 0.5rem;
        font-size: 1.25rem;
      }
      form {
        display: flex;
        flex-direction: column;
        gap: 0.85rem;
        margin-top: 1rem;
      }
      label {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        font-size: 0.85rem;
        color: var(--muted);
      }
      input {
        font-size: 1rem;
        padding: 0.55rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
      }
      input:focus {
        outline: 2px solid var(--accent);
        outline-offset: 1px;
      }
      .warn {
        color: var(--danger);
        font-size: 0.85rem;
        margin: 0;
      }
      .muted {
        color: var(--muted);
      }
      .small {
        font-size: 0.8rem;
        margin-top: 0.25rem;
      }
    `,
  ],
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
      this.errorMsg.set(formatError(e));
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
      this.errorMsg.set(formatError(e));
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
      this.errorMsg.set(formatError(e));
    } finally {
      this.busy.set(false);
    }
  }
}

function formatError(e: unknown): string {
  if (isBackendError(e)) {
    if (e.kind === 'wrong_password') return 'Incorrect master password.';
    if (e.kind === 'validation') return e.message.replace(/^validation:\s*/, '');
    return e.message;
  }
  return e instanceof Error ? e.message : String(e);
}
