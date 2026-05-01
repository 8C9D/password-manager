import { Component, inject, OnInit } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';

@Component({
  selector: 'app-category-sidebar',
  standalone: true,
  imports: [RouterLink],
  template: `
    <div class="sidebar">
      <h2>Categories</h2>
      <button
        type="button"
        class="cat"
        [class.active]="categories.selected() === null"
        (click)="select(null)"
      >
        All entries
      </button>
      @for (c of categories.categories(); track c.id) {
        <button
          type="button"
          class="cat"
          [class.active]="categories.selected() === c.id"
          (click)="select(c.id)"
        >
          {{ c.name }}
        </button>
      }
      <a routerLink="/vault/categories" class="manage">Manage categories…</a>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        height: 100%;
      }
      .sidebar {
        padding: 1rem 0.5rem;
      }
      h2 {
        font-size: 0.75rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--muted);
        margin: 0 0.4rem 0.5rem;
      }
      .cat {
        display: block;
        width: 100%;
        text-align: left;
        padding: 0.4rem 0.6rem;
        border-radius: 4px;
        font-size: 0.9rem;
        background: none;
        border: none;
        color: var(--text);
        cursor: pointer;
      }
      .cat:hover {
        background: var(--hover);
      }
      .cat.active {
        background: var(--accent-soft);
        color: var(--accent-strong);
        font-weight: 500;
      }
      .manage {
        display: block;
        margin-top: 0.5rem;
        padding: 0.4rem 0.6rem;
        font-size: 0.8rem;
        color: var(--muted);
        text-decoration: none;
      }
      .manage:hover {
        text-decoration: underline;
      }
    `,
  ],
})
export class CategorySidebarComponent implements OnInit {
  protected readonly categories = inject(CategoryService);

  async ngOnInit(): Promise<void> {
    await this.categories.list();
  }

  protected select(id: number | null): void {
    this.categories.select(id);
  }
}
