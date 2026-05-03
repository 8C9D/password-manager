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
  template: `
    <div class="shell">
      <header class="head">
        <h2>Settings</h2>
        <a routerLink="/vault" class="btn small">Back</a>
      </header>

      @if (loading()) {
        <p class="muted">Loading…</p>
      } @else {
        <section class="section">
          <h3>Auto-lock</h3>
          <p class="muted small">
            Vault will lock automatically after this many seconds of inactivity. Minimum 30 seconds.
          </p>
          <div class="presets">
            @for (p of presets; track p.secs) {
              <button
                type="button"
                class="btn small"
                [class.primary]="autoLockSecs === p.secs"
                (click)="autoLockSecs = p.secs"
              >
                {{ p.label }}
              </button>
            }
          </div>
          <label class="custom">
            Custom (seconds)
            <input
              type="number"
              [(ngModel)]="autoLockSecs"
              min="30"
              max="86400"
              name="autoLockSecs"
            />
          </label>

          @if (errorMsg()) {
            <p class="warn">{{ errorMsg() }}</p>
          }
          @if (savedMsg()) {
            <p class="ok">{{ savedMsg() }}</p>
          }
          <div class="actions">
            <button type="button" class="btn primary" [disabled]="busy()" (click)="onSave()">
              {{ busy() ? 'Saving…' : 'Save' }}
            </button>
          </div>
        </section>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .shell {
        padding: 1.25rem 1.5rem;
        max-width: 600px;
      }
      .head {
        display: flex;
        gap: 0.5rem;
        align-items: center;
        margin-bottom: 1rem;
      }
      .head h2 {
        flex: 1;
        margin: 0;
      }
      .section {
        display: flex;
        flex-direction: column;
        gap: 0.6rem;
      }
      .section h3 {
        margin: 0;
        font-size: 1rem;
      }
      .presets {
        display: flex;
        gap: 0.4rem;
        flex-wrap: wrap;
      }
      .custom {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        font-size: 0.85rem;
        color: var(--muted);
        max-width: 180px;
      }
      .custom input {
        padding: 0.5rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
      }
      .actions {
        display: flex;
        justify-content: flex-end;
      }
      .warn {
        color: var(--danger);
        font-size: 0.85rem;
        margin: 0;
      }
      .ok {
        color: var(--accent-strong);
        font-size: 0.85rem;
        margin: 0;
      }
      .muted {
        color: var(--muted);
      }
      .small {
        font-size: 0.85rem;
      }
    `,
  ],
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
