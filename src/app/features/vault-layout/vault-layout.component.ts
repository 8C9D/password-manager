import {
  Component,
  ElementRef,
  inject,
  OnDestroy,
  signal,
  viewChild,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink, RouterOutlet } from '@angular/router';

import { ClipboardService } from '../../core/services/clipboard.service';
import { ConfirmService } from '../../core/services/confirm.service';
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
export class VaultLayoutComponent implements OnDestroy {
  protected readonly vault = inject(VaultService);
  private readonly router = inject(Router);
  protected readonly clipboard = inject(ClipboardService);
  protected readonly entries = inject(PasswordEntryService);
  private readonly confirmSvc = inject(ConfirmService);

  protected readonly lockError = signal<string | null>(null);
  private readonly searchBox = viewChild<ElementRef<HTMLInputElement>>('searchBox');

  private readonly onKeydown = (e: KeyboardEvent) => {
    const action = shortcutFor(
      e.key,
      e.ctrlKey || e.metaKey,
      isTextEntryTarget(e.target),
      this.confirmSvc.state().open,
    );
    if (action === null) return;
    e.preventDefault();
    switch (action) {
      case 'focus-search':
        this.searchBox()?.nativeElement.focus();
        break;
      case 'new-entry':
        void this.router.navigate(['/vault/new']);
        break;
      case 'lock':
        void this.onLock();
        break;
    }
  };

  constructor() {
    document.addEventListener('keydown', this.onKeydown);
  }

  ngOnDestroy(): void {
    document.removeEventListener('keydown', this.onKeydown);
  }

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

export type VaultShortcut = 'focus-search' | 'new-entry' | 'lock';

/**
 * Map a keydown to a vault-wide shortcut, or null for keys we leave alone.
 *
 * `editing` suppresses only the bare `/`, which would otherwise be swallowed
 * while the user types a slash into a URL or notes field. The modifier
 * combinations stay live everywhere, so Lock in particular is always one
 * chord away no matter which field has focus.
 *
 * `dialogOpen` suppresses the two navigating shortcuts: a modal is meant to be
 * answered, and moving the page (or focus) out from under it strands the
 * dialog over an unrelated screen. Lock survives because locking dismisses the
 * dialog on its way out, and must never become unreachable.
 */
export function shortcutFor(
  key: string,
  modifier: boolean,
  editing: boolean,
  dialogOpen = false,
): VaultShortcut | null {
  if (dialogOpen) {
    return modifier && key.toLowerCase() === 'l' ? 'lock' : null;
  }
  if (modifier) {
    switch (key.toLowerCase()) {
      case 'k':
        return 'focus-search';
      case 'n':
        return 'new-entry';
      case 'l':
        return 'lock';
      default:
        return null;
    }
  }
  return key === '/' && !editing ? 'focus-search' : null;
}

/** Whether an event target is a field the user is typing into. */
export function isTextEntryTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.tagName !== 'string') return false;
  return (
    el.isContentEditable === true ||
    ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)
  );
}
