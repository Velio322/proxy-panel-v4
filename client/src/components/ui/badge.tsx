import type { HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/core-utils';

const badgeStyles = cva(
  'inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium leading-tight transition-colors',
  {
    variants: {
      variant: {
        default: 'border-primary/30 bg-primary/15 text-primary',
        secondary: 'border-border/60 bg-secondary/90 text-secondary-foreground',
        destructive: 'border-red-500/30 bg-red-500/15 text-red-600 dark:text-red-400',
        success: 'border-emerald-500/30 bg-emerald-500/15 text-emerald-600 dark:text-emerald-400',
        warning: 'border-amber-500/30 bg-amber-500/15 text-amber-600 dark:text-amber-400',
        outline: 'border-border/80 bg-transparent text-foreground',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
);

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeStyles> {}

export function Badge({ className, variant, ...rest }: BadgeProps) {
  return <span className={cn(badgeStyles({ variant }), className)} {...rest} />;
}
