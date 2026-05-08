import { Component, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink, RouterOutlet } from '@angular/router';

import { ClipboardService } from '../../core/services/clipboard.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { VaultService } from '../../core/services/vault.service';
import { CategorySidebarComponent } from '../category-sidebar/category-sidebar.component';
import { EntryListComponent } from '../entry-list/entry-list.component';

@Component({
  selector: 'app-vault-layout',
  standalone: true,
  imports: [
    RouterOutlet,
    RouterLink,
    FormsModule,
    CategorySidebarComponent,
    EntryListComponent,
  ],
  template: `
    <div class="layout">
      <header class="topbar">
        <h1>Password Manager</h1>
        <input
          type="search"
          placeholder="Search title / username / URL"
          class="search"
          [ngModel]="entries.searchQuery()"
          (ngModelChange)="entries.searchQuery.set($event)"
          name="search"
        />
        <div class="spacer"></div>
        <a routerLink="/vault/new" class="btn primary">+ Add entry</a>
        <a routerLink="/vault/settings" class="btn" title="Settings">⚙</a>
        <button class="btn" type="button" (click)="onLock()">Lock</button>
      </header>
      @if (clipboard.lastCopiedLabel(); as label) {
        <div class="clipboard-banner">
          Copied {{ label }} — auto-clears in {{ clipboard.clearAfterSecs() }}s.
        </div>
      }
      <div class="cols">
        <app-category-sidebar />
        <section class="list-pane">
          <app-entry-list />
        </section>
        <section class="detail-pane">
          <router-outlet />
        </section>
      </div>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        height: 100vh;
      }
      .layout {
        display: grid;
        grid-template-rows: auto auto 1fr;
        height: 100%;
        background: var(--bg);
      }
      .topbar {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.65rem 1rem;
        background: var(--surface);
        border-bottom: 1px solid var(--border);
      }
      .topbar h1 {
        margin: 0;
        font-size: 1rem;
        font-weight: 600;
        margin-right: 0.5rem;
      }
      .search {
        flex: 0 1 360px;
        padding: 0.4rem 0.7rem;
        border: 1px solid var(--border);
        border-radius: 6px;
        background: var(--bg);
        color: var(--text);
        font-size: 0.9rem;
      }
      .search:focus {
        outline: 2px solid var(--accent);
        outline-offset: 1px;
      }
      .spacer {
        flex: 1;
      }
      .clipboard-banner {
        background: #fff7e0;
        border-bottom: 1px solid #f1d588;
        color: #6a4d00;
        padding: 0.4rem 1rem;
        font-size: 0.85rem;
      }
      .cols {
        display: grid;
        grid-template-columns: 220px 320px 1fr;
        overflow: hidden;
      }
      app-category-sidebar,
      .list-pane {
        border-right: 1px solid var(--border);
        background: var(--surface);
        overflow-y: auto;
      }
      .detail-pane {
        overflow-y: auto;
      }
    `,
  ],
})
export class VaultLayoutComponent {
  private readonly vault = inject(VaultService);
  private readonly router = inject(Router);
  protected readonly clipboard = inject(ClipboardService);
  protected readonly entries = inject(PasswordEntryService);

  protected async onLock(): Promise<void> {
    await this.vault.lock();
    await this.router.navigate(['/unlock']);
  }
}
