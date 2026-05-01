import { Injectable, signal } from '@angular/core';

import { Category } from '../models/category.model';
import { call } from './tauri-invoke';

export type CategoryFilter = number | null;

@Injectable({ providedIn: 'root' })
export class CategoryService {
  readonly categories = signal<Category[]>([]);
  readonly selected = signal<CategoryFilter>(null);

  async list(): Promise<Category[]> {
    const rows = await call<Category[]>('list_categories');
    this.categories.set(rows);
    return rows;
  }

  async create(name: string): Promise<number> {
    const id = await call<number>('create_category', { name });
    await this.list();
    return id;
  }

  async rename(id: number, name: string): Promise<void> {
    await call<void>('update_category', { id, name });
    await this.list();
  }

  async remove(id: number): Promise<void> {
    await call<void>('delete_category', { id });
    if (this.selected() === id) this.selected.set(null);
    await this.list();
  }

  select(filter: CategoryFilter): void {
    this.selected.set(filter);
  }

  clear(): void {
    this.categories.set([]);
    this.selected.set(null);
  }
}
