import { Component, effect, inject } from '@angular/core';

import { ConfirmService } from '../../core/services/confirm.service';

@Component({
  selector: 'app-confirm-dialog',
  standalone: true,
  templateUrl: './confirm-dialog.component.html',
  styleUrl: './confirm-dialog.component.css',
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
