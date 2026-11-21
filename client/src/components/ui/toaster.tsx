import * as RadixToast from '@radix-ui/react-toast';
import { AlertCircle, AlertTriangle, CheckCircle2, Info, X } from 'lucide-react';
import { useToast, type ToastVariant } from '@/hooks/use-toast';
import { cn } from '@/lib/core-utils';

function ToastIcon({ variant }: { variant?: ToastVariant }) {
  switch (variant) {
    case 'destructive':
      return <AlertCircle className="h-4 w-4 shrink-0 text-red-400" />;
    case 'warning':
      return <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />;
    case 'success':
      return <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />;
    default:
      return <Info className="h-4 w-4 shrink-0 text-primary" />;
  }
}

export function Toaster() {
  const { toasts, dismiss } = useToast();

  return (
    <RadixToast.Provider swipeDirection="right">
      {toasts.map((item) => (
        <RadixToast.Root
          key={item.id}
          duration={item.duration}
          onOpenChange={(open) => {
            if (!open) dismiss(item.id);
          }}
          className={cn(
            'group pointer-events-auto relative flex w-full items-start gap-3 overflow-hidden rounded-xl border bg-card p-4 text-card-foreground shadow-xl',
            item.variant === 'destructive' && 'border-red-500/40',
            item.variant === 'warning' && 'border-amber-500/40',
            item.variant === 'success' && 'border-emerald-500/40',
          )}
        >
          <div className="mt-0.5">
            <ToastIcon variant={item.variant} />
          </div>
          <div className="flex-1 space-y-1">
            {item.title && <RadixToast.Title className="text-sm font-semibold">{item.title}</RadixToast.Title>}
            {item.description && (
              <RadixToast.Description className="text-xs text-muted-foreground">
                {item.description}
              </RadixToast.Description>
            )}
          </div>
          <RadixToast.Close
            aria-label="Close notification"
            className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <X className="h-4 w-4" />
          </RadixToast.Close>
        </RadixToast.Root>
      ))}
      <RadixToast.Viewport className="fixed bottom-4 right-4 z-[100] flex max-h-screen w-full max-w-sm flex-col gap-2 p-2 outline-none" />
    </RadixToast.Provider>
  );
}
