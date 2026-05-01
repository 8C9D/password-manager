import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';

import { VaultService } from '../services/vault.service';

export const unlockedGuard: CanActivateFn = async () => {
  const vault = inject(VaultService);
  const router = inject(Router);
  const status = await vault.refreshStatus();
  if (status.unlocked) return true;
  return router.parseUrl('/unlock');
};
