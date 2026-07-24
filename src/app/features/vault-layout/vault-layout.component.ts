import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink, RouterOutlet } from '@angular/router';

import { ClipboardService } from '../../core/services/clipboard.service';
import { PasswordEntryService } from '../../core/services/password-entry.service';
import { formatBackendError } from '../../core/services/tauri-invoke';
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
  templateUrl: './vault-layout.component.html',
  styleUrl: './vault-layout.component.css',
})
export class VaultLayoutComponent {
  protected readonly vault = inject(VaultService);
  private readonly router = inject(Router);
  protected readonly clipboard = inject(ClipboardService);
  protected readonly entries = inject(PasswordEntryService);

  protected readonly lockError = signal<string | null>(null);

  protected async onLock(): Promise<void> {
    this.lockError.set(null);
    try {
      await this.vault.lock();
    } catch (e) {
      // A silently-failed lock is worse than a failed unlock: the user may
      // walk away believing the vault is secured.
      this.lockError.set(formatBackendError(e));
      return;
    }
    await this.router.navigate(['/unlock']);
  }
}
