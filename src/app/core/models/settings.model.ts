/** The accepted range and default for one numeric setting. */
export interface Bound {
  min: number;
  max: number;
  default: number;
}

/**
 * Bounds published by the backend, which is the single source of truth for
 * them. The form validates and renders against these rather than repeating the
 * numbers on this side of the IPC boundary.
 */
export interface SettingsBounds {
  autoLockSecs: Bound;
  clipboardClearSecs: Bound;
  passwordHistoryLimit: Bound;
}

export interface AppSettings {
  autoLockSecs: number;
  clipboardClearSecs: number;
  /** Previous passwords retained per entry; 0 turns history off entirely. */
  passwordHistoryLimit: number;
}
