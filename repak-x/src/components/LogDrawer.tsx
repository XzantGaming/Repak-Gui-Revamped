import React, { useState, useEffect, useRef, useMemo } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { FaTerminal } from 'react-icons/fa'
import { VscClearAll } from 'react-icons/vsc'
import { BiCopyAlt } from 'react-icons/bi'
import { FiDownload, FiSearch, FiX } from 'react-icons/fi'
import { invoke } from '@tauri-apps/api/core'
import { formatLogLine, formatLogTime, type LogEntry, type LogLevel } from '../utils/uiLog'
import './LogDrawer.css'

type CopyFeedback = {
  id: string
  text: string
}

/**
 * Level filter presets.
 *
 * `normal` deliberately includes debug: with no third preset above it, anything
 * left out here would be unreachable from the UI, and the step-by-step detail
 * is the part a bug report needs. It renders dimmed instead of hidden, so it
 * stays out of the way without disappearing.
 */
type LevelFilter = 'normal' | 'problems'

const LEVEL_FILTERS: { id: LevelFilter; label: string; levels: LogLevel[] }[] = [
  { id: 'normal', label: 'Normal', levels: ['debug', 'info', 'success', 'warn', 'error'] },
  { id: 'problems', label: 'Problems', levels: ['warn', 'error'] }
]

type LogDrawerProps = {
  status?: string
  logs?: LogEntry[]
  onClear?: () => void
  defaultHeight?: number
  minHeight?: number
  maxHeightPercent?: number
  progress?: number
  isLoading?: boolean
  isOpen: boolean
  onToggle: () => void
}

/**
 * LogDrawer - a terminal-style drawer over the structured log bus.
 *
 * Entries carry their own level and scope (see `utils/uiLog.ts`), so styling,
 * filtering and the unread badge all read real data instead of substring
 * matching the message text.
 *
 * @param {Object} props
 * @param {string} [props.status] - Current status text to display in header
 * @param {LogEntry[]} [props.logs] - Log entries, oldest first
 * @param {function} [props.onClear] - Callback when clear button is clicked
 * @param {number} [props.defaultHeight=380] - Default height when expanded
 * @param {number} [props.minHeight=160] - Minimum drawer height
 * @param {number} [props.maxHeightPercent=0.85] - Maximum height as percentage of viewport
 * @param {number} [props.progress=0] - Progress value (0-100), or -1 for indeterminate
 * @param {boolean} [props.isLoading=false] - Whether a long operation is in progress
 */
export default function LogDrawer({
  status = 'Idle',
  logs = [],
  onClear,
  defaultHeight = 380,
  minHeight = 160,
  maxHeightPercent = 0.85,
  progress = 0,
  isLoading = false,
  isOpen,
  onToggle
}: LogDrawerProps) {
  const [drawerHeight, setDrawerHeight] = useState(defaultHeight)
  const [copyFeedback, setCopyFeedback] = useState<CopyFeedback | null>(null)
  const [levelFilter, setLevelFilter] = useState<LevelFilter>('normal')
  const [search, setSearch] = useState('')
  const [isExporting, setIsExporting] = useState(false)
  // Problems that arrived while the drawer was shut, so the badge can say what
  // the user missed rather than just how many lines scrolled past.
  const [unseenProblems, setUnseenProblems] = useState(0)
  const resizingRef = useRef(false)
  const logScrollRef = useRef<HTMLDivElement | null>(null)
  // Tracked by entry id rather than by count: the bus caps its history, so once
  // entries start rolling off the front an index would silently point at the
  // wrong place and the badge would miss problems.
  const lastSeenIdRef = useRef<string | null>(logs.length ? logs[logs.length - 1].id : null)

  const activeLevels = useMemo(
    () => new Set(LEVEL_FILTERS.find(f => f.id === levelFilter)?.levels ?? []),
    [levelFilter]
  )

  const visibleLogs = useMemo(() => {
    const needle = search.trim().toLowerCase()
    return logs.filter(entry => {
      if (!activeLevels.has(entry.level)) return false
      // Search covers the scope as well as the message, so typing a subsystem
      // name still narrows the drawer down to it.
      if (needle &&
        !entry.message.toLowerCase().includes(needle) &&
        !entry.scope.toLowerCase().includes(needle)) return false
      return true
    })
  }, [logs, activeLevels, search])

  const hiddenCount = logs.length - visibleLogs.length

  // Track problems that arrive while the drawer is collapsed.
  useEffect(() => {
    const newestId = logs.length ? logs[logs.length - 1].id : null

    if (isOpen) {
      lastSeenIdRef.current = newestId
      setUnseenProblems(0)
      return
    }
    if (newestId === lastSeenIdRef.current) return

    // A missing marker means those entries have rolled out of the buffer, so
    // everything still held counts as unseen.
    const seenIndex = lastSeenIdRef.current
      ? logs.findIndex(e => e.id === lastSeenIdRef.current)
      : -1
    const fresh = logs.slice(seenIndex + 1)
    lastSeenIdRef.current = newestId

    const problems = fresh.filter(e => e.level === 'warn' || e.level === 'error').length
    if (problems > 0) setUnseenProblems(n => n + problems)
  }, [logs, isOpen])

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    if (logScrollRef.current && isOpen) {
      logScrollRef.current.scrollTop = logScrollRef.current.scrollHeight
    }
  }, [visibleLogs, isOpen])

  // Handle resize drag
  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!resizingRef.current) return
      const y = e.clientY
      const vh = window.innerHeight
      const newH = Math.min(Math.max(vh - y, minHeight), Math.round(vh * maxHeightPercent))
      setDrawerHeight(newH)
    }
    const stop = () => { resizingRef.current = false }

    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', stop)
    window.addEventListener('mouseleave', stop)

    return () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', stop)
      window.removeEventListener('mouseleave', stop)
    }
  }, [minHeight, maxHeightPercent])

  const showFeedback = (id: string, text: string) => {
    setCopyFeedback({ id, text })
    setTimeout(() => setCopyFeedback(null), 1500)
  }

  const handleCopyLine = async (entry: LogEntry, e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    try {
      await navigator.clipboard.writeText(formatLogLine(entry))
      showFeedback(entry.id, 'Copied!')
    } catch (err) {
      console.error('Failed to copy to clipboard:', err)
    }
  }

  const handleCopyAll = async () => {
    if (visibleLogs.length === 0) return
    try {
      await navigator.clipboard.writeText(visibleLogs.map(formatLogLine).join('\n'))
      showFeedback('all', `${visibleLogs.length} line(s) copied`)
    } catch (err) {
      console.error('Failed to copy all logs:', err)
    }
  }

  /** Write the visible lines to a file next to the executable, then reveal it. */
  const handleExport = async () => {
    if (visibleLogs.length === 0 || isExporting) return
    setIsExporting(true)
    try {
      const path = await invoke<string>('export_ui_logs', {
        lines: visibleLogs.map(formatLogLine)
      })
      showFeedback('all', 'Saved to file')
      // Revealing it is a convenience, not the point of the export.
      await invoke('open_in_explorer', { path }).catch(() => { })
    } catch (err) {
      console.error('Failed to export logs:', err)
      showFeedback('all', 'Export failed')
    } finally {
      setIsExporting(false)
    }
  }

  const headerCount = unseenProblems > 0
    ? `${unseenProblems} problem${unseenProblems !== 1 ? 's' : ''}`
    : `${logs.length} log${logs.length !== 1 ? 's' : ''}`

  return (
    <motion.div
      className="log-drawer"
      animate={{ height: isOpen ? drawerHeight : 36 }}
      transition={{ type: 'tween', duration: 0.25 }}
    >
      <div
        className={`log-drawer-header ${isLoading ? 'is-loading' : ''}`}
        onClick={onToggle}
      >
        {/* Progress bar as background */}
        {isLoading && (
          <div className="log-drawer-progress-bg">
            <div
              className={`log-drawer-progress-bar ${progress < 0 ? 'indeterminate' : ''}`}
              style={progress >= 0 ? { width: `${progress}%` } : undefined}
            />
          </div>
        )}
        <div className="log-drawer-status">
          <FaTerminal className="log-drawer-icon" />
          <span className="log-drawer-status-text">{status}</span>
          {!isOpen && logs.length > 0 && (
            <span className={`log-drawer-count ${unseenProblems > 0 ? 'has-problems' : ''}`}>
              {headerCount}
            </span>
          )}
        </div>
        <div
          className="log-drawer-actions"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="log-drawer-btn"
            onClick={onToggle}
          >
            {isOpen ? 'Hide ▼' : 'Show ▲'}
          </button>
        </div>
      </div>

      {isOpen && (
        <div
          className="log-drawer-resize-handle"
          onMouseDown={(e: React.MouseEvent<HTMLDivElement>) => {
            e.stopPropagation()
            resizingRef.current = true
          }}
          title="Drag to resize"
        />
      )}

      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            className="log-drawer-body"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 12 }}
            transition={{ duration: 0.2 }}
          >
            <div className="log-drawer-toolbar">
              <div className="log-drawer-chip-group">
                {LEVEL_FILTERS.map(filter => (
                  <button
                    key={filter.id}
                    className={`log-drawer-chip ${levelFilter === filter.id ? 'active' : ''}`}
                    onClick={() => setLevelFilter(filter.id)}
                    title={
                      filter.id === 'problems'
                        ? 'Warnings and errors only'
                        : 'Everything'
                    }
                  >
                    {filter.label}
                  </button>
                ))}
              </div>

              <div className="log-drawer-search">
                <FiSearch className="log-drawer-search-icon" />
                <input
                  type="text"
                  value={search}
                  placeholder="Filter..."
                  onChange={(e) => setSearch(e.target.value)}
                  spellCheck={false}
                />
                {search && (
                  <button
                    className="log-drawer-search-clear"
                    onClick={() => setSearch('')}
                    title="Clear filter"
                  >
                    <FiX />
                  </button>
                )}
              </div>

              <div className="log-drawer-controls">
                {copyFeedback?.id === 'all' && (
                  <span className="log-drawer-feedback">{copyFeedback.text}</span>
                )}
                {logs.length > 0 && (
                  <>
                    <button
                      className="log-drawer-action-btn"
                      onClick={handleCopyAll}
                      title="Copy visible lines"
                    >
                      <BiCopyAlt />
                    </button>
                    <button
                      className="log-drawer-action-btn"
                      onClick={handleExport}
                      disabled={isExporting}
                      title="Save visible lines to a file"
                    >
                      <FiDownload />
                    </button>
                    {onClear && (
                      <button
                        className="log-drawer-action-btn"
                        onClick={onClear}
                        title="Clear logs"
                      >
                        <VscClearAll />
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>

            {logs.length === 0 ? (
              <div className="log-drawer-empty">
                <span className="log-drawer-prompt">$</span>
                <span className="log-drawer-waiting">Waiting for output...</span>
                <span className="log-drawer-cursor" />
              </div>
            ) : visibleLogs.length === 0 ? (
              <div className="log-drawer-empty">
                <span className="log-drawer-prompt">$</span>
                <span className="log-drawer-waiting">
                  Nothing matches the current filter ({logs.length} line{logs.length !== 1 ? 's' : ''} hidden)
                </span>
              </div>
            ) : (
              <>
                <div className="log-drawer-scroll" ref={logScrollRef}>
                  {visibleLogs.map((entry, i) => (
                    <div
                      key={entry.id}
                      className={`log-drawer-line ${entry.level}`}
                      onContextMenu={(e) => handleCopyLine(entry, e)}
                      title="Right-click to copy line"
                    >
                      <span className="log-drawer-line-number">{String(i + 1).padStart(3, ' ')}</span>
                      <span className="log-drawer-line-time">{formatLogTime(entry.ts)}</span>
                      <span className="log-drawer-line-scope">{entry.scope}</span>
                      <span className="log-drawer-line-content">{entry.message}</span>
                      {copyFeedback?.id === entry.id && (
                        <span className="log-line-feedback">Copied!</span>
                      )}
                    </div>
                  ))}
                </div>
                {hiddenCount > 0 && (
                  <div className="log-drawer-hidden-note">
                    {hiddenCount} line{hiddenCount !== 1 ? 's' : ''} hidden by filters
                  </div>
                )}
              </>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  )
}
