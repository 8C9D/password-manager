import { Injectable, signal } from '@angular/core';

export interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

interface DialogState {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  danger: boolean;
}

const CLOSED: DialogState = {
  open: false,
  title: '',
  message: '',
  confirmLabel: 'Confirm',
  cancelLabel: 'Cancel',
  danger: false,
};

@Injectable({ providedIn: 'root' })
export class ConfirmService {
  readonly state = signal<DialogState>(CLOSED);
  private resolveFn: ((ok: boolean) => void) | null = null;

  ask(request: ConfirmRequest): Promise<boolean> {
    if (this.resolveFn !== null) {
      this.resolveFn(false);
      this.resolveFn = null;
    }
    return new Promise<boolean>((resolve) => {
      this.resolveFn = resolve;
      this.state.set({
        open: true,
        title: request.title,
        message: request.message,
        confirmLabel: request.confirmLabel ?? 'Confirm',
        cancelLabel: request.cancelLabel ?? 'Cancel',
        danger: request.danger ?? false,
      });
    });
  }

  resolve(ok: boolean): void {
    const fn = this.resolveFn;
    this.resolveFn = null;
    this.state.set(CLOSED);
    fn?.(ok);
  }
}
