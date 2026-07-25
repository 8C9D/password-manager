import { Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';

import { AutoLockService } from './core/services/auto-lock.service';
import { ThemeService } from './core/services/theme.service';
import { ConfirmDialogComponent } from './features/confirm-dialog/confirm-dialog.component';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, ConfirmDialogComponent],
  templateUrl: './app.html',
  styleUrl: './app.css',
})
export class App {
  constructor() {
    inject(AutoLockService);
    // Constructed here so the stored theme is applied on the first frame,
    // including on the unlock screen.
    inject(ThemeService);
  }
}
