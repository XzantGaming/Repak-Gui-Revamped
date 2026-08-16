import { useCallback, useLayoutEffect, useRef } from 'react'

/**
 * Give a callback a permanently stable identity while always invoking the
 * latest version of it.
 *
 * Handlers written as plain arrows in a component body get a new identity on
 * every render, which defeats `memo` on any child they are passed to — for a
 * list, that means every row re-renders whenever anything in the parent
 * changes. Wrapping with `useCallback` fixes the identity but requires an
 * exhaustive dependency list, and a missed dependency turns into a stale-state
 * bug that is far worse than the re-render.
 *
 * This is the "latest ref" / event-callback pattern: the returned function
 * never changes, and it forwards to whatever the handler is at call time.
 * Only use it for event handlers — never call the result during render, where
 * reading the ref could observe a value from the previous commit.
 */
export function useStableCallback<Args extends unknown[], Result>(
  fn: (...args: Args) => Result,
): (...args: Args) => Result {
  const ref = useRef(fn)

  useLayoutEffect(() => {
    ref.current = fn
  })

  return useCallback((...args: Args) => ref.current(...args), [])
}
