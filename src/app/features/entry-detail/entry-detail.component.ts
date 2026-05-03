import { Component, inject, signal } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';

import { EntryFull } from '../../core/models/entry.model';
import { CategoryService } from '../../core/services/category.service';
import { ClipboardService } from '../../core/services/clipboard.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-entry-detail',
  standalone: true,
  imports: [RouterLink],
  template: `
    <div class="detail">
      @if (loading()) {
        <p class="muted">Loading…</p>
      } @else if (error()) {
        <p class="warn">{{ error() }}</p>
      } @else if (entry(); as e) {
        <header class="header">
          <h2>{{ e.title }}</h2>
          <div class="header-actions">
            <a [routerLink]="['/vault', e.id, 'edit']" class="btn small">Edit</a>
            <button type="button" class="btn small danger" (click)="onDelete(e.id, e.title)">
              Delete
            </button>
          </div>
        </header>

        <dl>
          @if (categoryName(); as cat) {
            <dt>Category</dt>
            <dd><span class="value">{{ cat }}</span></dd>
          }

          <dt>Username</dt>
          <dd>
            <span class="value">{{ e.username || '—' }}</span>
            @if (e.username) {
              <button class="btn small" type="button" (click)="copy(e.username, 'username')">Copy</button>
            }
          </dd>

          <dt>URL / App</dt>
          <dd>
            <span class="value">{{ e.urlOrAppName || '—' }}</span>
          </dd>

          <dt>Password</dt>
          <dd>
            <span class="value mono">
              {{ showPassword() ? e.password : mask(e.password) }}
            </span>
            <button class="btn small" type="button" (click)="toggleShow()">
              {{ showPassword() ? 'Hide' : 'Show' }}
            </button>
            <button class="btn small primary" type="button" (click)="copy(e.password, 'password')">
              Copy
            </button>
          </dd>

          @if (e.notes) {
            <dt>Notes</dt>
            <dd>
              <pre class="notes">{{ e.notes }}</pre>
            </dd>
          }

          <dt>Created</dt>
          <dd><span class="value muted">{{ formatDate(e.createdAt) }}</span></dd>

          <dt>Last viewed</dt>
          <dd><span class="value muted">{{ e.lastUsedAt ? formatDate(e.lastUsedAt) : 'just now' }}</span></dd>
        </dl>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        height: 100%;
      }
      .detail {
        padding: 1.25rem 1.5rem;
        max-width: 720px;
      }
      .header {
        display: flex;
        align-items: center;
        margin-bottom: 1rem;
        gap: 0.5rem;
      }
      .header h2 {
        flex: 1;
        margin: 0;
      }
      .header-actions {
        display: flex;
        gap: 0.4rem;
      }
      .btn.danger {
        color: var(--danger);
        border-color: var(--danger);
      }
      .btn.danger:hover {
        background: rgba(185, 28, 28, 0.08);
      }
      dl {
        display: grid;
        grid-template-columns: 130px 1fr;
        row-gap: 0.6rem;
        column-gap: 1rem;
        margin: 0;
      }
      dt {
        font-size: 0.78rem;
        text-transform: uppercase;
        color: var(--muted);
        letter-spacing: 0.03em;
        padding-top: 0.45rem;
      }
      dd {
        margin: 0;
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        align-items: center;
      }
      .value {
        flex: 1 1 auto;
        word-break: break-all;
      }
      .mono {
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, monospace;
      }
      .muted {
        color: var(--muted);
      }
      .warn {
        color: var(--danger);
      }
      .notes {
        flex: 1 1 100%;
        margin: 0;
        padding: 0.6rem 0.8rem;
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 6px;
        font-family: inherit;
        white-space: pre-wrap;
      }
    `,
  ],
})
export class EntryDetailComponent {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly entries = inject(PasswordEntryService);
  private readonly clipboard = inject(ClipboardService);
  private readonly categoriesSvc = inject(CategoryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected readonly entry = signal<EntryFull | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly showPassword = signal(false);

  constructor() {
    this.route.paramMap.pipe(takeUntilDestroyed()).subscribe((params) => {
      const raw = params.get('id');
      const id = raw === null ? NaN : Number(raw);
      if (!Number.isFinite(id) || id <= 0) {
        this.error.set('Invalid entry id.');
        this.loading.set(false);
        return;
      }
      this.load(id);
    });
  }

  protected categoryName(): string | null {
    const e = this.entry();
    if (!e || e.categoryId === null) return null;
    const cat = this.categoriesSvc.categories().find((c) => c.id === e.categoryId);
    return cat?.name ?? null;
  }

  private async load(id: number): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    this.entry.set(null);
    this.showPassword.set(false);
    try {
      const e = await this.entries.get(id);
      this.entry.set(e);
    } catch (err) {
      this.error.set(formatBackendError(err));
    } finally {
      this.loading.set(false);
    }
  }

  protected toggleShow(): void {
    this.showPassword.update((v) => !v);
  }

  protected async copy(value: string, label: string): Promise<void> {
    await this.clipboard.copy(value, label);
  }

  protected mask(value: string): string {
    return '•'.repeat(Math.min(value.length, 24));
  }

  protected formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }

  protected async onDelete(id: number, title: string): Promise<void> {
    const ok = await this.confirmSvc.ask({
      title: 'Delete entry?',
      message: `"${title}" will be permanently deleted. This cannot be undone.`,
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    try {
      await this.entries.remove(id);
      await this.router.navigate(['/vault']);
    } catch (e) {
      this.error.set(formatBackendError(e));
    }
  }
}
