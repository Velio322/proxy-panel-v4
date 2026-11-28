import { useCallback, useEffect, useRef, useState } from 'react';

type CopyFn = (text: string, key?: string) => Promise<void>;

export function useCopyFeedback(
  resetMs = 1500,
): [string | null, CopyFn, () => void] {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    };
  }, []);

  const reset = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (mountedRef.current) setCopiedKey(null);
  }, []);

  const copy = useCallback(
    async (text: string, key = '') => {
      try {
        if (navigator.clipboard?.writeText) {
          await navigator.clipboard.writeText(text);
        } else {
          const ta = document.createElement('textarea');
          ta.value = text;
          ta.style.position = 'fixed';
          ta.style.opacity = '0';
          document.body.appendChild(ta);
          ta.select();
          document.execCommand('copy');
          document.body.removeChild(ta);
        }
        if (!mountedRef.current) return;
        setCopiedKey(key);
        if (timerRef.current !== null) window.clearTimeout(timerRef.current);
        timerRef.current = window.setTimeout(() => {
          if (mountedRef.current) setCopiedKey(null);
        }, resetMs);
      } catch {
        // Clipboard permission denied or insecure context — do not crash caller.
      }
    },
    [resetMs],
  );

  return [copiedKey, copy, reset];
}
