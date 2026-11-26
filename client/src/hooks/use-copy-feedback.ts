import { useCallback, useEffect, useRef, useState } from 'react';

type CopyFn = (text: string, key?: string) => Promise<void>;

export function useCopyFeedback(
  resetMs = 1500,
): [string | null, CopyFn, () => void] {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const reset = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    setCopiedKey(null);
  }, []);

  const copy = useCallback(
    async (text: string, key = '') => {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => setCopiedKey(null), resetMs);
    },
    [resetMs],
  );

  return [copiedKey, copy, reset];
}
