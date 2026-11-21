import { cn } from '@/lib/core-utils';

export function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      aria-hidden="true"
      className={cn('animate-pulse rounded-md bg-muted/60', className)}
      {...props}
    />
  );
}
