export interface EntrySummary {
  id: number;
  categoryId: number | null;
  title: string;
  username: string;
  urlOrAppName: string;
  createdAt: string;
  updatedAt: string;
  lastUsedAt: string | null;
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
}

export interface EntryInput {
  categoryId: number | null;
  title: string;
  username: string;
  urlOrAppName: string;
  password: string;
  notes: string | null;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
  vaultName: string | null;
}
