/**
 * Hero portrait store.
 *
 * Portraits are no longer bundled with the app — they live in the
 * rivals-resources repo and are cached in %APPDATA%/Repak-X/hero. This module
 * loads that cache once, keeps it in memory as `id -> blob URL`, and lets
 * components read it synchronously the way the old `import.meta.glob` map did.
 *
 * Because the cache arrives asynchronously, components that render portraits
 * should call `useHeroImages()` so they re-render when the set lands.
 */
import { invoke } from '@tauri-apps/api/core'
import { useSyncExternalStore } from 'react'

export type CharacterDataEntry = {
  name: string
  id: string
}

export type HeroSyncResult = {
  cached: number
  added: string[]
  updated: string[]
  checked: boolean
  message: string
}

/** Portrait shown for unknown / multiple-hero mods. */
export const FALLBACK_HERO_ID = '9999'

let images: Record<string, string> = {}
let version = 0
let initPromise: Promise<void> | null = null

const listeners = new Set<() => void>()

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function notify() {
  version += 1
  listeners.forEach(listener => listener())
}

/**
 * Normalize an IPC binary payload into something `Blob` accepts.
 *
 * A Rust command returning `tauri::ipc::Response` arrives as an ArrayBuffer,
 * but the exact shape is decided by the injected runtime rather than by the
 * JS package, so accept the typed-array and plain-array forms too — passing a
 * number[] straight to `Blob` would stringify it into garbage.
 */
function toBytes(payload: ArrayBuffer | Uint8Array | number[]): BlobPart {
  if (payload instanceof ArrayBuffer) return payload
  return new Uint8Array(payload)
}

/**
 * Pull whatever is on disk into memory as blob URLs. Safe to call repeatedly.
 *
 * Blob URLs rather than data URLs: the bytes cross IPC raw instead of as
 * base64, each portrait decodes once no matter how many rows show it, and the
 * DOM holds a short `blob:` URL instead of a ~46 KB string per `<img>` and
 * per `background-image`.
 */
async function loadCache(): Promise<number> {
  try {
    const ids = await invoke<string[]>('get_hero_image_ids')
    if (!ids || ids.length === 0) return 0

    const loaded: Record<string, string> = {}
    await Promise.all(
      ids.map(async id => {
        try {
          const bytes = await invoke<ArrayBuffer | Uint8Array | number[]>('get_hero_image', { id })
          loaded[id] = URL.createObjectURL(new Blob([toBytes(bytes)], { type: 'image/png' }))
        } catch (e) {
          console.warn(`[Hero] Failed to load portrait ${id}:`, e)
        }
      }),
    )

    // Release the previous generation only after the new one is built, so
    // rendered <img> elements never point at a revoked URL.
    const stale = Object.values(images)
    images = loaded
    notify()
    stale.forEach(url => URL.revokeObjectURL(url))

    return Object.keys(images).length
  } catch (e) {
    console.warn('[Hero] Failed to load cached portraits:', e)
    return 0
  }
}

/**
 * Load cached portraits, then reconcile the cache with the repo.
 *
 * The cache is shown first so a warm start paints immediately and never waits
 * on the network; the sync only repaints when it actually pulled something.
 */
export async function initHeroImages(): Promise<void> {
  if (!initPromise) {
    initPromise = (async () => {
      const cachedCount = await loadCache()

      try {
        const result = await invoke<HeroSyncResult>('sync_hero_images', { force: false })
        if (result.added.length > 0 || result.updated.length > 0 || cachedCount === 0) {
          await loadCache()
        }
        if (!result.checked) {
          console.warn('[Hero] Portrait sync skipped:', result.message)
        }
      } catch (e) {
        // Offline or GitHub unreachable — the cached set stays usable.
        console.warn('[Hero] Portrait sync failed:', e)
      }
    })()
  }
  return initPromise
}

/**
 * Force a fresh check against the repo, ignoring the stored ETag.
 * Returns the sync result so callers can report what changed.
 */
export async function refreshHeroImages(): Promise<HeroSyncResult | null> {
  try {
    const result = await invoke<HeroSyncResult>('sync_hero_images', { force: true })
    await loadCache()
    return result
  } catch (e) {
    console.warn('[Hero] Portrait refresh failed:', e)
    return null
  }
}

/** Blob URL for a character id's portrait, or undefined when it is not cached. */
export function getHeroImageById(id?: string | null): string | undefined {
  if (!id) return undefined
  return images[id]
}

/**
 * Resolve a portrait from a hero name, preferring a direct character id.
 *
 * `fallbackId` is used when nothing matches — pass `FALLBACK_HERO_ID` at call
 * sites that want the placeholder, omit it to render no image at all.
 */
export function resolveHeroImage(
  heroName?: string | null,
  characterData: CharacterDataEntry[] = [],
  characterId?: string | null,
  fallbackId?: string,
): string | undefined {
  const fallback = fallbackId ? getHeroImageById(fallbackId) : undefined

  // Direct ID lookup (preferred)
  const byId = getHeroImageById(characterId)
  if (byId) return byId

  // Fallback for missing, Unknown, or Multiple Heroes
  if (!heroName) return fallback
  const lowered = heroName.toLowerCase()
  if (lowered.includes('unknown') || lowered.includes('multiple')) return fallback

  // Fallback: find by base hero name in character data
  const baseName = heroName.includes(' - ') ? heroName.split(' - ')[0] : heroName
  const char = (characterData || []).find(c => c.name === baseName)
  const byName = getHeroImageById(char?.id)
  if (byName) return byName

  return fallback
}

/**
 * Subscribe to the portrait store. The returned number is only a change token —
 * components render through `resolveHeroImage` / `getHeroImageById`, and this
 * hook exists so they re-render once the cache finishes loading (including
 * inside `memo`-wrapped list items).
 */
export function useHeroImages(): number {
  return useSyncExternalStore(subscribe, () => version, () => version)
}
