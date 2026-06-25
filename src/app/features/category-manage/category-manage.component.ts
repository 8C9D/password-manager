import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';

import { Category } from '../../core/models/category.model';
import { CategoryService } from '../../core/services/category.service';
import { ConfirmService } from '../../core/services/confirm.service';
import { formatBackendError } from '../../core/services/tauri-invoke';

@Component({
  selector: 'app-category-manage',
  standalone: true,
  imports: [FormsModule, RouterLink],
  templateUrl: './category-manage.component.html',
  styleUrl: './category-manage.component.css',
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
      this.errorMsg.set(formatBackendError(e));
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
      this.errorMsg.set(formatBackendError(e));
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
      this.errorMsg.set(formatBackendError(e));
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
      this.errorMsg.set(formatBackendError(e));
    } finally {
      this.busy.set(false);
    }
  }
}
