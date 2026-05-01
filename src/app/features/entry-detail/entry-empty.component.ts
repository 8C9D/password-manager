import { Component } from '@angular/core';

@Component({
  selector: 'app-entry-empty',
  standalone: true,
  template: `
    <div class="empty">
      <p>Select an entry to view its details.</p>
      <p class="muted small">Or click "+ Add entry" to create one.</p>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
        height: 100%;
      }
      .empty {
        height: 100%;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 0.3rem;
        text-align: center;
        padding: 2rem;
      }
      .muted {
        color: var(--muted);
      }
      .small {
        font-size: 0.85rem;
      }
    `,
  ],
})
export class EntryEmptyComponent {}
