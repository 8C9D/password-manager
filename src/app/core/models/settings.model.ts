export interface AppSettings {
  autoLockSecs: number;
  clipboardClearSecs: number;
  /** Previous passwords retained per entry; 0 turns history off entirely. */
  passwordHistoryLimit: number;
}
