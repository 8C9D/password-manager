import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { Category } from '../../core/models/category.model';
import { CategoryService } from '../../core/services/category.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { isBackendError } from '../../core/services/tauri-invoke';
import { PasswordGeneratorPanelComponent } from '../password-generator/password-generator-panel.component';

@Component({
  selector: 'app-entry-form',
  standalone: true,
  imports: [FormsModule, RouterLink, PasswordGeneratorPanelComponent],
  template: `
    <div class="form-shell">
      <header class="form-head">
        <h2>{{ isEdit() ? 'Edit entry' : 'New entry' }}</h2>
        <a [routerLink]="cancelLink()" class="btn small">Cancel</a>
      </header>

      @if (loading()) {
        <p class="muted">Loading…</p>
      } @else {
        <form (ngSubmit)="onSubmit()" class="form">
          <label>
            Title<span class="req">*</span>
            <input type="text" name="title" [(ngModel)]="title" required autofocus />
          </label>
          @if (titleTouched() && title.trim() === '') {
            <p class="warn">Title is required.</p>
          }

          <label>
            Category
            <select name="categoryId" [(ngModel)]="categoryId">
              <option [ngValue]="null">(uncategorized)</option>
              @for (c of categories(); track c.id) {
                <option [ngValue]="c.id">{{ c.name }}</option>
              }
            </select>
          </label>

          <label>
            Username / email
            <input type="text" name="username" [(ngModel)]="username" autocomplete="off" />
          </label>

          <label>
            URL or app name
            <input type="text" name="urlOrAppName" [(ngModel)]="urlOrAppName" autocomplete="off" />
          </label>

          <label>
            Password<span class="req">*</span>
            <div class="pw-row">
              <input
                [type]="showPassword() ? 'text' : 'password'"
                name="password"
                [(ngModel)]="password"
                autocomplete="new-password"
                required
              />
              <button type="button" class="btn small" (click)="toggleShow()">
                {{ showPassword() ? 'Hide' : 'Show' }}
              </button>
              <button type="button" class="btn small" (click)="toggleGenerator()">
                {{ showGenerator() ? 'Hide generator' : 'Generate' }}
              </button>
            </div>
          </label>
          @if (passwordTouched() && password === '') {
            <p class="warn">Password is required.</p>
          }
          @if (showGenerator()) {
            <app-password-generator-panel (accept)="onGenerated($event)" />
          }

          <label>
            Notes
            <textarea name="notes" [(ngModel)]="notes" rows="4"></textarea>
          </label>

          @if (errorMsg()) {
            <p class="warn">{{ errorMsg() }}</p>
          }

          <div class="actions">
            <a [routerLink]="cancelLink()" class="btn">Cancel</a>
            <button type="submit" class="btn primary" [disabled]="busy()">
              {{ busy() ? 'Saving…' : (isEdit() ? 'Save changes' : 'Save entry') }}
            </button>
          </div>
        </form>
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .form-shell {
        padding: 1.25rem 1.5rem;
        max-width: 600px;
      }
      .form-head {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-bottom: 1rem;
      }
      .form-head h2 {
        margin: 0;
        flex: 1;
      }
      .form {
        display: flex;
        flex-direction: column;
        gap: 0.85rem;
      }
      label {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        font-size: 0.85rem;
        color: var(--muted);
      }
      input,
      textarea,
      select {
        font-size: 0.95rem;
        padding: 0.5rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
        font-family: inherit;
      }
      input:focus,
      textarea:focus,
      select:focus {
        outline: 2px solid var(--accent);
        outline-offset: 1px;
      }
      .pw-row {
        display: flex;
        gap: 0.4rem;
      }
      .pw-row input {
        flex: 1;
      }
      .actions {
        display: flex;
        justify-content: flex-end;
        gap: 0.5rem;
        margin-top: 0.5rem;
      }
      .req {
        color: var(--danger);
        margin-left: 2px;
      }
      .warn {
        color: var(--danger);
        font-size: 0.85rem;
        margin: -0.3rem 0 0;
      }
      .muted {
        color: var(--muted);
      }
    `,
  ],
})
export class EntryFormComponent implements OnInit {
  private readonly entries = inject(PasswordEntryService);
  private readonly categoriesSvc = inject(CategoryService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected title = '';
  protected username = '';
  protected urlOrAppName = '';
  protected password = '';
  protected notes = '';
  protected categoryId: number | null = null;

  protected readonly busy = signal(false);
  protected readonly loading = signal(true);
  protected readonly errorMsg = signal<string | null>(null);
  protected readonly showPassword = signal(false);
  protected readonly showGenerator = signal(false);
  protected readonly titleTouched = signal(false);
  protected readonly passwordTouched = signal(false);
  protected readonly categories = signal<Category[]>([]);
  protected readonly editingId = signal<number | null>(null);

  protected isEdit = () => this.editingId() !== null;
  protected cancelLink = () => {
    const id = this.editingId();
    return id ? ['/vault', id] : ['/vault'];
  };

  async ngOnInit(): Promise<void> {
    const idParam = this.route.snapshot.paramMap.get('id');
    const id = idParam ? Number(idParam) : null;
    this.editingId.set(id);
    try {
      await this.categoriesSvc.list();
      this.categories.set(this.categoriesSvc.categories());
      if (id !== null && Number.isFinite(id) && id > 0) {
        const full = await this.entries.get(id);
        this.title = full.title;
        this.username = full.username;
        this.urlOrAppName = full.urlOrAppName;
        this.password = full.password;
        this.notes = full.notes ?? '';
        this.categoryId = full.categoryId;
      }
    } catch (e) {
      this.errorMsg.set(formatError(e));
    } finally {
      this.loading.set(false);
    }
  }

  protected toggleShow(): void {
    this.showPassword.update((v) => !v);
  }

  protected toggleGenerator(): void {
    this.showGenerator.update((v) => !v);
  }

  protected onGenerated(pw: string): void {
    this.password = pw;
    this.passwordTouched.set(true);
    this.showGenerator.set(false);
  }

  protected async onSubmit(): Promise<void> {
    this.titleTouched.set(true);
    this.passwordTouched.set(true);
    if (this.title.trim() === '' || this.password === '' || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      const input = {
        categoryId: this.categoryId,
        title: this.title.trim(),
        username: this.username,
        urlOrAppName: this.urlOrAppName,
        password: this.password,
        notes: this.notes === '' ? null : this.notes,
      };
      const id = this.editingId();
      if (id !== null) {
        await this.entries.update(id, input);
        await this.router.navigate(['/vault', id]);
      } else {
        const newId = await this.entries.create(input);
        await this.router.navigate(['/vault', newId]);
      }
    } catch (e) {
      this.errorMsg.set(formatError(e));
    } finally {
      this.busy.set(false);
    }
  }
}

function formatError(e: unknown): string {
  if (isBackendError(e)) {
    if (e.kind === 'validation') return e.message.replace(/^validation:\s*/, '');
    if (e.kind === 'locked') return 'Vault is locked.';
    if (e.kind === 'entry_not_found') return 'Entry not found.';
    return e.message;
  }
  return e instanceof Error ? e.message : String(e);
}
