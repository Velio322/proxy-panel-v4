import * as React from 'react';

export type ToastVariant = 'default' | 'destructive' | 'success' | 'warning';

export interface ToastItem {
  id: string;
  title?: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
}

type Listener = (toasts: ToastItem[]) => void;

const listeners = new Set<Listener>();
let memoryToasts: ToastItem[] = [];
let toastSeq = 0;

function notify() {
  for (const listener of listeners) {
    listener([...memoryToasts]);
  }
}

export function dismissToast(id: string) {
  memoryToasts = memoryToasts.filter((t) => t.id !== id);
  notify();
}

export function toast(input: Omit<ToastItem, 'id'>) {
  toastSeq += 1;
  const id = `toast-${toastSeq}`;
  const item: ToastItem = {
    duration: 4500,
    variant: 'default',
    ...input,
    id,
  };
  memoryToasts = [item, ...memoryToasts].slice(0, 5);
  notify();
  return {
    id,
    dismiss: () => dismissToast(id),
  };
}

export function useToast() {
  const [toasts, setToasts] = React.useState<ToastItem[]>(memoryToasts);

  React.useEffect(() => {
    listeners.add(setToasts);
    return () => {
      listeners.delete(setToasts);
    };
  }, []);

  return {
    toasts,
    toast,
    dismiss: dismissToast,
  };
}
