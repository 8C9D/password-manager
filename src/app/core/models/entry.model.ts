export interface EntrySummary {
  id: number;
  categoryId: number | null;
  title: string;
  username: string;
  urlOrAppName: string;
  createdAt: string;
  updatedAt: string;
  lastUsedAt: string | null;
  favorite: boolean;
  tags: string[];
}

/**
 * A user-defined extra field on an entry. The value is encrypted at rest like
 * a password; `secret` only decides whether the UI masks it.
 */
export interface CustomField {
  label: string;
  value: string;
  secret: boolean;
}

export interface EntryFull {
  id: number;
  categoryId: number | null;
  title: string;
  username: string;
  urlOrAppName: string;
  password: string;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
  lastUsedAt: string | null;
  hasTotp: boolean;
  favorite: boolean;
  tags: string[];
  /** Days between password changes before a rotation reminder fires. */
  passwordExpiryDays: number | null;
  /** When the password is next due for rotation, or null with no reminder. */
  passwordDueAt: string | null;
  fields: CustomField[];
}

/** How an entry write should treat the stored TOTP secret. */
export type TotpUpdate =
  | { action: 'keep' }
  | { action: 'clear' }
  | { action: 'set'; value: string };

export interface EntryInput {
  categoryId: number | null;
  title: string;
  username: string;
  urlOrAppName: string;
  password: string;
  notes: string | null;
  totp?: TotpUpdate;
  favorite?: boolean;
  tags?: string[];
  passwordExpiryDays?: number | null;
  fields?: CustomField[];
}

/** An entry sitting in the trash: restorable until it is purged. */
export interface DeletedEntry {
  id: number;
  title: string;
  username: string;
  urlOrAppName: string;
  deletedAt: string;
}

export interface PasswordHistoryItem {
  id: number;
  password: string;
  /** When this password stopped being the entry's current one. */
  changedAt: string;
}

export interface GeneratedTotp {
  code: string;
  period: number;
  secondsRemaining: number;
}

export interface EntryIssue {
  id: number;
  title: string;
  weak: boolean;
  reused: boolean;
  stale: boolean;
  due: boolean;
}

export interface VaultHealth {
  total: number;
  weakCount: number;
  reusedCount: number;
  staleCount: number;
  dueCount: number;
  issues: EntryIssue[];
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
  vaultName: string | null;
}
