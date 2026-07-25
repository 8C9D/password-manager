import { Routes } from '@angular/router';

import { unlockedGuard } from './core/guards/unlocked.guard';
import { CategoryManageComponent } from './features/category-manage/category-manage.component';
import { EntryDetailComponent } from './features/entry-detail/entry-detail.component';
import { EntryEmptyComponent } from './features/entry-detail/entry-empty.component';
import { EntryFormComponent } from './features/entry-form/entry-form.component';
import { SettingsComponent } from './features/settings/settings.component';
import { VaultHealthComponent } from './features/vault-health/vault-health.component';
import { VaultLayoutComponent } from './features/vault-layout/vault-layout.component';
import { VaultTrashComponent } from './features/vault-trash/vault-trash.component';
import { VaultUnlockComponent } from './features/vault-unlock/vault-unlock.component';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'unlock' },
  { path: 'unlock', component: VaultUnlockComponent },
  {
    path: 'vault',
    component: VaultLayoutComponent,
    canActivate: [unlockedGuard],
    children: [
      { path: '', component: EntryEmptyComponent },
      { path: 'new', component: EntryFormComponent },
      { path: 'categories', component: CategoryManageComponent },
      { path: 'health', component: VaultHealthComponent },
      { path: 'settings', component: SettingsComponent },
      { path: 'trash', component: VaultTrashComponent },
      { path: ':id', component: EntryDetailComponent },
      { path: ':id/edit', component: EntryFormComponent },
    ],
  },
  { path: '**', redirectTo: 'unlock' },
];
