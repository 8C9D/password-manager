import { Component, effect, inject } from '@angular/core';

import { ConfirmService } from '../../core/services/confirm.service';

@Component({
  selector: 'app-confirm-dialog',
  standalone: true,
  template: `
    @if (state().open) {
      <div class="backdrop" (click)="cancel()">
        <div
          class="dialog"
          role="dialog"
          aria-modal="true"
          (click)="$event.stopPropagation()"
        >
          <h3>{{ state().title }}</h3>
          <p>{{ state().message }}</p>
          <div class="actions">
            <button type="button" class="btn" (click)="cancel()">
              {{ state().cancelLabel }}
            </button>
            <button
              type="button"
              class="btn"
              [class.danger]="state().danger"
              [class.primary]="!state().danger"
              (click)="confirm()"
              autofocus
            >
              {{ state().confirmLabel }}
            </button>
          </div>
        </div>
      </div>
    }
  `,
  styles: [
    `
      .backdrop {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.45);
        display: grid;
        place-items: center;
        z-index: 1000;
      }
      .dialog {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 1.2rem 1.4rem;
        min-width: 320px;
        max-width: 480px;
        box-shadow: 0 4px 20px rgba(0, 0, 0, 0.18);
      }
      h3 {
        margin: 0 0 0.5rem;
        font-size: 1rem;
      }
      p {
        margin: 0 0 1rem;
        color: var(--text);
      }
      .actions {
        display: flex;
        justify-content: flex-end;
        gap: 0.5rem;
      }
      .btn.danger {
        background: var(--danger);
        color: white;
        border-color: var(--danger);
      }
      .btn.danger:hover {
        filter: brightness(1.05);
      }
    `,
  ],
})
export class ConfirmDialogComponent {
  private readonly svc = inject(ConfirmService);
  protected readonly state = this.svc.state;

  constructor() {
    effect((onCleanup) => {
      if (!this.state().open) return;
      const handler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') this.cancel();
        if (e.key === 'Enter') this.confirm();
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
