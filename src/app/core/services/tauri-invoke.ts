import { invoke } from '@tauri-apps/api/core';

export interface BackendError {
  kind: string;
  message: string;
}

export function isBackendError(value: unknown): value is BackendError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    'message' in value
  );
}

export type TauriCommand =
  | 'vault_status'
  | 'create_vault'
  | 'unlock_vault'
  | 'lock_vault'
  | 'create_entry'
  | 'list_entries'
  | 'get_entry'
  | 'update_entry'
  | 'delete_entry'
  | 'list_categories'
  | 'create_category'
  | 'update_category'
  | 'delete_category'
  | 'generate_password'
  | 'get_settings'
  | 'update_settings'
  | 'copy_to_clipboard';

export async function call<T>(
  command: TauriCommand,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}

export type BackendErrorOverrides = Partial<Record<string, string>>;

const VALIDATION_PREFIX = /^validation:\s*/;

const DEFAULT_MESSAGES: Record<string, string> = {
  locked: 'Vault is locked.',
  wrong_password: 'Incorrect master password.',
  entry_not_found: 'Entry not found.',
  category_not_found: 'Category not found.',
};

export function formatBackendError(
  e: unknown,
  overrides?: BackendErrorOverrides,
): string {
  if (isBackendError(e)) {
    const override = overrides?.[e.kind];
    if (override !== undefined) return override;
    if (e.kind === 'validation') return e.message.replace(VALIDATION_PREFIX, '');
    return DEFAULT_MESSAGES[e.kind] ?? e.message;
  }
  return e instanceof Error ? e.message : String(e);
}
