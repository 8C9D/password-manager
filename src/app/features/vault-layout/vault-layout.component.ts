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
  templateUrl: './vault-layout.component.html',
  styleUrl: './vault-layout.component.css',
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
