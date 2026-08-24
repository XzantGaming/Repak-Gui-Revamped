/**
 * The log drawer's single source of truth.
 *
 * Entries arrive from three places and all of them land here:
 *
 *  - `ui_log` events, the structured payload emitted by `src/ui_log.rs`.
 *  - `install_log` events, the older stringly-typed channel still used by the
 *    mod-detection path. Those are parsed back into a level and a scope so a
 *    detection run reads the same as everything else.
 *  - Frontend calls to `uiLog.*`, for work that never leaves the webview
 *    (update checks, changelog fetches, drag-and-drop).
 *
 * Keeping the entries in a module rather than in App state means any component
 * can log without prop drilling, and the drawer stays a pure renderer.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type LogLevel = 'debug' | 'info' | 'success' | 'warn' | 'error'

export type LogEntry = {
  /** Stable identity. Backend entries reuse their sequence number so an entry
   *  that arrives via both the replay buffer and the live listener collapses
   *  into one. */
  id: string
  /** Unix milliseconds. */
  ts: number
  level: LogLevel
  /** Short subsystem tag rendered as a chip, e.g. `CharDB`, `Heroes`. */
  scope: string
  message: string
}

/** Shape of the `ui_log` Tauri event payload (see `src/ui_log.rs`). */
type UiLogEvent = {
  seq: number
  ts: number
  level: LogLevel
  scope: string
  message: string
}

/**
 * Upper bound on retained entries. A long detection run emits a few thousand
 * lines; without a cap the array grew for the lifetime of the process and every
 * render walked all of it.
 */
const MAX_ENTRIES = 2000

let entries: LogEntry[] = []
let localSeq = 0
const seen = new Set<string>()
const listeners = new Set<(entries: LogEntry[]) => void>()

function notify() {
  for (const fn of listeners) fn(entries)
}

function push(entry: LogEntry) {
  // The frontend subscribes before draining the backlog, so the same backend
  // entry can legitimately arrive twice.
  if (seen.has(entry.id)) return
  seen.add(entry.id)

  entries = entries.length >= MAX_ENTRIES
    ? [...entries.slice(entries.length - MAX_ENTRIES + 1), entry]
    : [...entries, entry]

  notify()
}

/** Current entries. Stable identity between notifications. */
export function getLogEntries(): LogEntry[] {
  return entries
}

/** Subscribe to entry changes. Returns an unsubscribe function. */
export function subscribeToLogs(fn: (entries: LogEntry[]) => void): () => void {
  listeners.add(fn)
  return () => { listeners.delete(fn) }
}

/** Drop every entry. The dedupe set is cleared too, so a redisplay is possible. */
export function clearLogs() {
  entries = []
  seen.clear()
  notify()
}

/** Record a frontend-originated entry. */
export function log(level: LogLevel, scope: string, message: string) {
  push({
    id: `f${++localSeq}`,
    ts: Date.now(),
    level,
    scope,
    message
  })
}

export const uiLog = {
  debug: (scope: string, message: string) => log('debug', scope, message),
  info: (scope: string, message: string) => log('info', scope, message),
  success: (scope: string, message: string) => log('success', scope, message),
  warn: (scope: string, message: string) => log('warn', scope, message),
  error: (scope: string, message: string) => log('error', scope, message)
}

// ============================================================================
// LEGACY install_log BRIDGE
// ============================================================================

/**
 * Guess a level for a line that predates structured logging.
 *
 * This is the old `getLogClass` heuristic, kept only for `install_log`. It runs
 * on the message with its `[Scope]` prefix already stripped, so a path
 * containing the word "error" is still a false positive — but it no longer
 * competes with every structured entry in the drawer.
 */
function inferLevel(message: string): LogLevel {
  const lower = message.toLowerCase()
  if (lower.includes('✗') || lower.includes('error') || lower.includes('failed')) return 'error'
  if (lower.includes('warning') || lower.includes('warn')) return 'warn'
  if (lower.includes('✓') || lower.includes('success') || lower.includes('complete')) return 'success'
  return 'info'
}

/** Split a `[Scope] message` line, defaulting to the `Install` scope. */
function parseLegacyLine(raw: string): { scope: string; message: string } {
  const match = /^\s*\[([^\]]{1,24})\]\s*(.*)$/.exec(raw)
  if (!match) return { scope: 'Install', message: raw }
  return { scope: match[1].trim(), message: match[2] }
}

/**
 * Wire the bus to the backend: subscribe first, then replay the backlog.
 *
 * The order matters. `setup()` in Rust runs its startup checks long before the
 * webview exists, so those entries only survive in the replay buffer — and an
 * entry emitted between subscribing and draining would be lost the other way
 * around. Duplicates are cheaper than gaps, and `push` dedupes them.
 */
export async function initLogBridge(): Promise<UnlistenFn> {
  const unlistenStructured = await listen<UiLogEvent>('ui_log', (event) => {
    const p = event.payload
    push({
      id: `b${p.seq}`,
      ts: p.ts,
      level: p.level,
      scope: p.scope,
      message: p.message
    })
  })

  const unlistenLegacy = await listen('install_log', (event) => {
    const raw = String(event.payload)
    const { scope, message } = parseLegacyLine(raw)
    push({
      id: `f${++localSeq}`,
      ts: Date.now(),
      level: inferLevel(message),
      scope,
      message
    })
  })

  try {
    const backlog = await invoke<UiLogEvent[]>('get_ui_log_backlog')
    for (const p of backlog) {
      push({
        id: `b${p.seq}`,
        ts: p.ts,
        level: p.level,
        scope: p.scope,
        message: p.message
      })
    }
    // Sequence order is authoritative; the backlog can land after live entries.
    entries = [...entries].sort((a, b) => a.ts - b.ts)
    notify()
  } catch (e) {
    console.warn('[uiLog] Could not read startup log backlog:', e)
  }

  return () => {
    unlistenStructured()
    unlistenLegacy()
  }
}

// ============================================================================
// FORMATTING
// ============================================================================

/** `HH:MM:SS`, the drawer's per-line timestamp. */
export function formatLogTime(ts: number): string {
  const d = new Date(ts)
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map(n => String(n).padStart(2, '0'))
    .join(':')
}

/** One entry as a copyable/exportable line. */
export function formatLogLine(entry: LogEntry): string {
  return `[${formatLogTime(entry.ts)}] [${entry.level.toUpperCase()}] [${entry.scope}] ${entry.message}`
}
