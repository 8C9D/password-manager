import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';

import { CategoryService } from '../../core/services/category.service';
import {
  filterEntries,
  PasswordEntryService,
} from '../../core/services/password-entry.service';

@Component({
  selector: 'app-entry-list',
  standalone: true,
  imports: [RouterLink, RouterLinkActive],
  template: `
    <div class="list-container">
      @if (loading()) {
        <p class="muted">Loading…</p>
      } @else if (visible().length === 0) {
        @if (entries.entries().length === 0) {
          <div class="empty">
            <p>No entries yet.</p>
            <p class="muted small">Click "+ Add entry" to add your first one.</p>
          </div>
        } @else {
          <div class="empty">
            <p class="muted">No entries match this filter.</p>
          </div>
        }
      } @else {
        <ul>
          @for (entry of visible(); track entry.id) {
            <li>
              <a
                [routerLink]="['/vault', entry.id]"
                routerLinkActive="selected"
              >
                <span class="title">{{ entry.title }}</span>
                @if (entry.username) {
                  <span class="sub">{{ entry.username }}</span>
                }
              </a>
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
        height: 100%;
      }
      .list-container {
        padding: 0.5rem 0;
      }
      ul {
        list-style: none;
        margin: 0;
        padding: 0;
      }
      a {
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
        padding: 0.55rem 1rem;
        text-decoration: none;
        color: inherit;
        border-left: 3px solid transparent;
      }
      a:hover {
        background: var(--hover);
      }
      a.selected {
        background: var(--accent-soft);
        border-left-color: var(--accent);
        color: var(--accent-strong);
      }
      .title {
        font-weight: 500;
      }
      .sub {
        font-size: 0.8rem;
        color: var(--muted);
      }
      a.selected .sub {
        color: var(--accent-strong);
        opacity: 0.8;
      }
      .empty {
        padding: 1.5rem 1rem;
        text-align: center;
      }
      .muted {
        color: var(--muted);
      }
      .small {
        font-size: 0.8rem;
      }
    `,
  ],
})
export class EntryListComponent implements OnInit {
  protected readonly entries = inject(PasswordEntryService);
  protected readonly categories = inject(CategoryService);

  protected readonly loading = signal(true);

  protected readonly visible = computed(() =>
    filterEntries(
      this.entries.entries(),
      this.categories.selected(),
      this.entries.searchQuery(),
    ),
  );

  async ngOnInit(): Promise<void> {
    try {
      await this.entries.list();
    } finally {
      this.loading.set(false);
    }
  }
}
