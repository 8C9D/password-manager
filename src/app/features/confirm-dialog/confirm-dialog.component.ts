import {
  Component,
  effect,
  ElementRef,
  inject,
  viewChild,
} from '@angular/core';

import { ConfirmService } from '../../core/services/confirm.service';

/**
 * Focusable children of the dialog, in tab order. The dialog only ever holds
 * its two buttons, but querying keeps the trap correct if that changes.
 */
function focusableWithin(host: HTMLElement): HTMLElement[] {
  return Array.from(
    host.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    ),
  );
}

/**
 * Keep Tab inside the dialog, wrapping at both ends. Returns true when the
 * event was handled and the browser's own focus move must be suppressed.
 */
export function trapTabFocus(
  focusable: readonly HTMLElement[],
  active: Element | null,
  shiftKey: boolean,
): HTMLElement | null {
  if (focusable.length === 0) return null;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (shiftKey) return active === first ? last : null;
  return active === last ? first : null;
}

@Component({
  selector: 'app-confirm-dialog',
  standalone: true,
  templateUrl: './confirm-dialog.component.html',
  styleUrl: './confirm-dialog.component.css',
})
export class ConfirmDialogComponent {
  private readonly svc = inject(ConfirmService);
  protected readonly state = this.svc.state;

  private readonly dialogEl = viewChild<ElementRef<HTMLElement>>('dialog');
  private readonly confirmBtn = viewChild<ElementRef<HTMLElement>>('confirmBtn');

  constructor() {
    // `autofocus` does nothing here: the dialog is inserted by @if long after
    // the document is parsed, so focus has to be moved explicitly - and put
    // back afterwards, or answering a dialog strands the keyboard at the top
    // of the page.
    effect((onCleanup) => {
      const host = this.dialogEl()?.nativeElement;
      if (!host) return;
      const previous = document.activeElement as HTMLElement | null;
      this.confirmBtn()?.nativeElement.focus();
      onCleanup(() => previous?.focus?.());
    });

    effect((onCleanup) => {
      const host = this.dialogEl()?.nativeElement;
      if (!host) return;
      const handler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          this.cancel();
          return;
        }
        if (e.key === 'Tab') {
          const target = trapTabFocus(
            focusableWithin(host),
            document.activeElement,
            e.shiftKey,
          );
          if (target) {
            e.preventDefault();
            target.focus();
          }
          return;
        }
        if (e.key === 'Enter') {
          // A focused button already activates itself on Enter. Handling it
          // here as well would confirm the dialog even when Cancel has focus.
          if ((e.target as HTMLElement | null)?.tagName === 'BUTTON') return;
          e.preventDefault();
          this.confirm();
        }
      };
      document.addEventListener('keydown', handler);
      onCleanup(() => document.removeEventListener('keydown', handler));
    });
  }

  protected confirm(): void {
    this.svc.resolve(true);
  }

  protected cancel(): void {
    this.svc.resolve(false);
  }
}
