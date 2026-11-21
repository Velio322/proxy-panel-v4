import React from 'react';
import { LayoutGrid, List } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/core-utils';

export interface PageHeroProps {
  title: string;
  subtitle?: string;
  icon?: React.ReactNode;
  badge?: React.ReactNode;
  actions?: React.ReactNode;
  meta?: React.ReactNode;
  className?: string;
}

export function PageHero({
  title,
  subtitle,
  icon,
  badge,
  actions,
  meta,
  className,
}: PageHeroProps) {
  return (
    <div
      className={cn(
        'node-hero-grid relative overflow-hidden rounded-xl border border-border/80 bg-card/90 p-4 pl-5 sm:p-5 sm:pl-6',
        className,
      )}
    >
      <div
        className="absolute inset-y-0 left-0 w-1 bg-gradient-to-b from-emerald-400 via-primary to-cyan-400 shadow-[0_0_12px_hsl(162_84%_43%/0.6)]"
        aria-hidden="true"
      />
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2.5">
            {icon && (
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-primary/25 bg-primary/12 text-primary">
                {icon}
              </span>
            )}
            <h1 className="text-xl font-bold tracking-tight sm:text-2xl">{title}</h1>
            {badge}
          </div>
          {subtitle && <p className="text-xs text-muted-foreground max-w-3xl">{subtitle}</p>}
        </div>
        {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
      {meta && <div className="mt-3.5 border-t border-border/60 pt-3">{meta}</div>}
    </div>
  );
}

export interface RackEmptyStateProps {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export function RackEmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
}: RackEmptyStateProps) {
  return (
    <div
      className={cn(
        'rounded-xl border border-dashed border-border/80 bg-card/45 px-6 py-10 text-center',
        className,
      )}
    >
      <div className="mx-auto max-w-md space-y-3">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-xl border border-primary/25 bg-primary/10 text-primary shadow-sm">
          <Icon className="h-5 w-5" />
        </div>
        <div className="space-y-1">
          <div className="text-sm font-semibold text-foreground">{title}</div>
          {description && <p className="text-xs text-muted-foreground leading-relaxed">{description}</p>}
        </div>
        {action && <div className="pt-1 flex justify-center">{action}</div>}
      </div>
    </div>
  );
}

export interface StatusFilterPillItem {
  key: string;
  label: string;
  count?: number;
  dotClass?: string;
}

export interface StatusFilterPillsProps {
  items: StatusFilterPillItem[];
  value: string;
  onChange: (key: string) => void;
  className?: string;
}

export function StatusFilterPills({ items, value, onChange, className }: StatusFilterPillsProps) {
  return (
    <div className={cn('flex flex-wrap items-center gap-1.5', className)}>
      {items.map((item) => {
        const active = value === item.key;
        return (
          <button
            key={item.key}
            type="button"
            onClick={() => onChange(item.key)}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors',
              active
                ? 'border-primary/45 bg-primary/15 text-primary'
                : 'border-border/70 bg-background/40 text-muted-foreground hover:border-border hover:text-foreground',
            )}
          >
            {item.dotClass && <span className={cn('h-1.5 w-1.5 rounded-full', item.dotClass)} />}
            <span>{item.label}</span>
            {item.count !== undefined && (
              <span className="font-mono text-[10px] opacity-80 tabular-nums">{item.count}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}

export interface ViewModeToggleProps {
  value: 'blades' | 'table';
  onChange: (v: 'blades' | 'table') => void;
  isRu: boolean;
}

export function ViewModeToggle({ value, onChange, isRu }: ViewModeToggleProps) {
  return (
    <div className="inline-flex rounded-lg border border-border/80 bg-muted/40 p-0.5">
      <Button
        type="button"
        size="sm"
        variant={value === 'blades' ? 'secondary' : 'ghost'}
        className="h-7 gap-1 px-2.5 text-xs"
        onClick={() => onChange('blades')}
      >
        <LayoutGrid className="h-3.5 w-3.5" />
        <span>{isRu ? 'Карточки' : 'Blades'}</span>
      </Button>
      <Button
        type="button"
        size="sm"
        variant={value === 'table' ? 'secondary' : 'ghost'}
        className="h-7 gap-1 px-2.5 text-xs"
        onClick={() => onChange('table')}
      >
        <List className="h-3.5 w-3.5" />
        <span>{isRu ? 'Таблица' : 'Table'}</span>
      </Button>
    </div>
  );
}

export interface SplitBarSegment {
  pct: number;
  className: string;
}

export function SplitBar({
  segments,
  className,
}: {
  segments: SplitBarSegment[];
  className?: string;
}) {
  return (
    <div className={cn('telemetry-track flex h-1.5 w-full overflow-hidden rounded-full', className)}>
      {segments.map((seg, idx) =>
        seg.pct > 0 ? (
          <div
            key={idx}
            className={cn('h-full transition-all duration-300', seg.className)}
            style={{ width: `${Math.max(0, Math.min(100, seg.pct))}%` }}
          />
        ) : null,
      )}
    </div>
  );
}
