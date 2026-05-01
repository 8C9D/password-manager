import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';

import { Category } from '../../core/models/category.model';
import { CategoryService } from '../../core/services/category.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { isBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-category-manage',
  standalone: true,
  imports: [FormsModule, RouterLink],
  template: `
    <div class="shell">
      <header class="head">
        <h2>Categories</h2>
        <a routerLink="/vault" class="btn small">Back</a>
      </header>

      <form (ngSubmit)="onCreate()" class="new-row">
        <input
          type="text"
          name="newName"
          [(ngModel)]="newName"
          placeholder="New category name"
          maxlength="64"
        />
        <button type="submit" class="btn primary" [disabled]="busy() || newName.trim() === ''">
          Add
        </button>
      </form>
      @if (errorMsg()) {
        <p class="warn">{{ errorMsg() }}</p>
      }

      @if (categories.categories().length === 0) {
        <p class="muted">No categories yet.</p>
      } @else {
        <ul class="list">
          @for (c of categories.categories(); track c.id) {
            <li>
              @if (editingId() === c.id) {
                <input
                  type="text"
                  [(ngModel)]="editingName"
                  [ngModelOptions]="{ standalone: true }"
                  maxlength="64"
                />
                <button class="btn small primary" type="button" (click)="onRename(c.id)">Save</button>
                <button class="btn small" type="button" (click)="cancelEdit()">Cancel</button>
              } @else {
                <span class="name">{{ c.name }}</span>
                <button class="btn small" type="button" (click)="startEdit(c)">Rename</button>
                <button class="btn small danger" type="button" (click)="onDelete(c)">Delete</button>
              }
            </li>
          }
        </ul>
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
      .new-row {
        display: flex;
        gap: 0.5rem;
        margin-bottom: 1rem;
      }
      .new-row input {
        flex: 1;
        padding: 0.5rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
      }
      .list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
      }
      .list li {
        display: flex;
        gap: 0.5rem;
        align-items: center;
        padding: 0.5rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--surface);
      }
      .name {
        flex: 1;
      }
      .list input {
        flex: 1;
        padding: 0.35rem 0.55rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
      }
      .warn {
        color: var(--danger);
        font-size: 0.85rem;
      }
      .muted {
        color: var(--muted);
      }
      .btn.danger {
        color: var(--danger);
        border-color: var(--danger);
      }
    `,
  ],
})
export class CategoryManageComponent implements OnInit {
  protected readonly categories = inject(CategoryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected newName = '';
  protected editingName = '';
  protected readonly editingId = signal<number | null>(null);
  protected readonly busy = signal(false);
  protected readonly errorMsg = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    try {
      await this.categories.list();
    } catch (e) {
      this.errorMsg.set(formatError(e));
    }
  }

  protected async onCreate(): Promise<void> {
    if (this.newName.trim() === '' || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      await this.categories.create(this.newName.trim());
      this.newName = '';
    } catch (e) {
      this.errorMsg.set(formatError(e));
    } finally {
      this.busy.set(false);
    }
  }

  protected startEdit(c: Category): void {
    this.editingId.set(c.id);
    this.editingName = c.name;
    this.errorMsg.set(null);
  }

  protected cancelEdit(): void {
    this.editingId.set(null);
    this.editingName = '';
  }

  protected async onRename(id: number): Promise<void> {
    if (this.editingName.trim() === '' || this.busy()) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      await this.categories.rename(id, this.editingName.trim());
      this.cancelEdit();
    } catch (e) {
      this.errorMsg.set(formatError(e));
    } finally {
      this.busy.set(false);
    }
  }

  protected async onDelete(c: Category): Promise<void> {
    const ok = await this.confirmSvc.ask({
      title: 'Delete category?',
      message: `"${c.name}" will be removed. Entries in it become uncategorized.`,
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    this.busy.set(true);
    this.errorMsg.set(null);
    try {
      await this.categories.remove(c.id);
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
    if (e.kind === 'entry_not_found') return 'Category not found.';
    return e.message;
  }
  return e instanceof Error ? e.message : String(e);
}
