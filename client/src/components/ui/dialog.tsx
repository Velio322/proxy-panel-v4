import * as React from 'react';
import * as RadixDialog from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { cn } from '@/lib/core-utils';

export const Dialog = RadixDialog.Root;
export const DialogTrigger = RadixDialog.Trigger;
export const DialogPortal = RadixDialog.Portal;
export const DialogClose = RadixDialog.Close;

export const DialogOverlay = React.forwardRef<
  React.ElementRef<typeof RadixDialog.Overlay>,
  React.ComponentPropsWithoutRef<typeof RadixDialog.Overlay>
>(({ className, ...props }, ref) => (
  <RadixDialog.Overlay
    ref={ref}
    className={cn(
      'fixed inset-0 z-[60] bg-black/70 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0',
      className,
    )}
    {...props}
  />
));
DialogOverlay.displayName = 'DialogOverlay';

/**
 * Dialog widths follow the viewport instead of a fixed pixel cap: each size grows with the
 * screen between a floor and a ceiling and never exceeds the viewport minus a small gutter.
 * Heights use `dvh`, so mobile browser toolbars do not push the dialog off screen.
 */
export type DialogSize = 'sm' | 'md' | 'lg' | 'xl' | '2xl' | '3xl' | '4xl' | '5xl';

const DIALOG_SIZES: Record<DialogSize, string> = {
  sm: 'max-w-[min(calc(100vw-1.5rem),clamp(20rem,28vw,26rem))]',
  md: 'max-w-[min(calc(100vw-1.5rem),clamp(24rem,34vw,32rem))]',
  lg: 'max-w-[min(calc(100vw-1.5rem),clamp(28rem,40vw,42rem))]',
  xl: 'max-w-[min(calc(100vw-1.5rem),clamp(32rem,46vw,52rem))]',
  '2xl': 'max-w-[min(calc(100vw-1.5rem),clamp(36rem,54vw,64rem))]',
  '3xl': 'max-w-[min(calc(100vw-1.5rem),clamp(40rem,62vw,76rem))]',
  '4xl': 'max-w-[min(calc(100vw-1.5rem),clamp(44rem,70vw,92rem))]',
  '5xl': 'max-w-[min(calc(100vw-1.5rem),clamp(48rem,78vw,104rem))]',
};

export const DialogContent = React.forwardRef<
  React.ElementRef<typeof RadixDialog.Content>,
  React.ComponentPropsWithoutRef<typeof RadixDialog.Content> & { size?: DialogSize }
>(({ className, children, size = 'lg', ...props }, ref) => (
  <DialogPortal>
    <DialogOverlay />
    <RadixDialog.Content
      ref={ref}
      className={cn(
        'fixed left-1/2 top-1/2 z-[60] grid w-[calc(100%-1.5rem)] -translate-x-1/2 -translate-y-1/2 gap-4 rounded-xl border bg-card p-4 text-card-foreground shadow-2xl max-h-[calc(100dvh-1.5rem)] overflow-y-auto focus:outline-none sm:p-6 sm:max-h-[90dvh]',
        DIALOG_SIZES[size],
        className,
      )}
      {...props}
    >
      {children}
      <RadixDialog.Close
        aria-label="Close"
        className="absolute right-4 top-4 rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <X className="h-4 w-4" />
      </RadixDialog.Close>
    </RadixDialog.Content>
  </DialogPortal>
));
DialogContent.displayName = 'DialogContent';

export function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('flex flex-col space-y-1.5 pr-6 text-left', className)} {...props} />;
}

export function DialogFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('flex flex-col-reverse gap-2 pt-2 sm:flex-row sm:justify-end', className)}
      {...props}
    />
  );
}

export const DialogTitle = React.forwardRef<
  React.ElementRef<typeof RadixDialog.Title>,
  React.ComponentPropsWithoutRef<typeof RadixDialog.Title>
>(({ className, ...props }, ref) => (
  <RadixDialog.Title
    ref={ref}
    className={cn('text-base font-semibold leading-none tracking-tight', className)}
    {...props}
  />
));
DialogTitle.displayName = 'DialogTitle';

export const DialogDescription = React.forwardRef<
  React.ElementRef<typeof RadixDialog.Description>,
  React.ComponentPropsWithoutRef<typeof RadixDialog.Description>
>(({ className, ...props }, ref) => (
  <RadixDialog.Description
    ref={ref}
    className={cn('text-xs text-muted-foreground', className)}
    {...props}
  />
));
DialogDescription.displayName = 'DialogDescription';
