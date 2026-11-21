import { useId } from 'react';
import { cn } from '@/lib/core-utils';

interface SparklineProps {
  values: number[];
  width?: number;
  height?: number;
  stroke?: string;
  id?: string;
  className?: string;
}

export function Sparkline({
  values,
  width = 128,
  height = 28,
  stroke = '#10b981',
  id,
  className,
}: SparklineProps) {
  const autoId = useId().replace(/:/g, '');
  const gradId = `spark-${id ? id.replace(/[^a-zA-Z0-9_-]/g, '') : autoId}`;

  const clean = values
    .filter((v) => Number.isFinite(v))
    .map((v) => Math.max(0, Math.min(100, v)));

  if (clean.length === 0) {
    return (
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        className={cn('overflow-visible opacity-40', className)}
        aria-hidden="true"
      >
        <line
          x1={2}
          y1={height / 2}
          x2={width - 2}
          y2={height / 2}
          stroke="currentColor"
          strokeDasharray="3 3"
          strokeWidth={1}
        />
      </svg>
    );
  }

  const pts = clean.length === 1 ? [clean[0], clean[0]] : clean;
  const padY = 4;
  const innerH = Math.max(4, height - padY * 2);
  const stepX = (width - 4) / (pts.length - 1);

  const coords = pts.map((v, i) => {
    const x = 2 + i * stepX;
    const y = padY + (1 - v / 100) * innerH;
    return [Number(x.toFixed(1)), Number(y.toFixed(1))] as const;
  });

  const linePath = coords.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x} ${y}`).join(' ');
  const areaPath = `${linePath} L${coords[coords.length - 1][0]} ${height - 1} L${coords[0][0]} ${height - 1} Z`;
  const [lastX, lastY] = coords[coords.length - 1];

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={cn('overflow-visible', className)}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={gradId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity={0.32} />
          <stop offset="100%" stopColor={stroke} stopOpacity={0.0} />
        </linearGradient>
      </defs>
      <path d={areaPath} fill={`url(#${gradId})`} />
      <path d={linePath} fill="none" stroke={stroke} strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" />
      <circle cx={lastX} cy={lastY} r={2.4} fill={stroke} />
    </svg>
  );
}

interface RadialGaugeProps {
  value: number | null | undefined;
  tone?: 'emerald' | 'cyan';
  size?: number;
  label?: string;
}

export function RadialGauge({ value, tone = 'emerald', size = 72, label }: RadialGaugeProps) {
  const valid = value !== null && value !== undefined && Number.isFinite(value);
  const clamped = valid ? Math.max(0, Math.min(100, value)) : 0;

  const r = 27;
  const cx = 36;
  const cy = 36;
  const circumference = 2 * Math.PI * r;
  const arcFraction = 0.75; // 270 deg gauge
  const arcLen = circumference * arcFraction;
  const filledLen = arcLen * (clamped / 100);

  const strokeColor =
    clamped >= 90
      ? '#ef4444'
      : clamped >= 75
        ? '#f59e0b'
        : tone === 'cyan'
          ? '#06b6d4'
          : '#10b981';

  return (
    <div className="relative inline-flex shrink-0 items-center justify-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox="0 0 72 72" aria-hidden="true">
        <g transform="rotate(135 36 36)">
          <circle
            cx={cx}
            cy={cy}
            r={r}
            fill="none"
            stroke="hsl(var(--muted))"
            strokeWidth={6}
            strokeDasharray={`${arcLen} ${circumference}`}
            strokeLinecap="round"
          />
          {valid && (
            <circle
              cx={cx}
              cy={cy}
              r={r}
              fill="none"
              stroke={strokeColor}
              strokeWidth={6}
              strokeDasharray={`${filledLen} ${circumference}`}
              strokeLinecap="round"
              style={{ transition: 'stroke-dasharray 300ms ease, stroke 300ms ease' }}
            />
          )}
        </g>
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center leading-none">
        <span className="font-mono text-sm font-bold tabular-nums">
          {valid ? `${clamped.toFixed(0)}%` : '—'}
        </span>
        {label && <span className="mt-0.5 text-[9px] uppercase tracking-wider text-muted-foreground">{label}</span>}
      </div>
    </div>
  );
}

interface MeterBarProps {
  value: number | null | undefined;
  tone?: 'emerald' | 'cyan';
  className?: string;
}

export function MeterBar({ value, tone = 'emerald', className }: MeterBarProps) {
  const valid = value !== null && value !== undefined && Number.isFinite(value);
  const clamped = valid ? Math.max(0, Math.min(100, value)) : 0;

  const fillClass =
    clamped >= 90
      ? 'bg-red-500'
      : clamped >= 75
        ? 'bg-amber-400'
        : tone === 'cyan'
          ? 'bg-cyan-400'
          : 'bg-emerald-400';

  return (
    <div className={cn('telemetry-track h-1.5 w-full overflow-hidden rounded-full', className)}>
      <div
        className={cn('h-full rounded-full transition-all duration-300', fillClass)}
        style={{ width: `${valid ? clamped : 0}%` }}
      />
    </div>
  );
}

export function formatUptime(seconds: number | null | undefined, isRu: boolean): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return '—';
  }
  const s = Math.floor(seconds);
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  const mins = Math.floor((s % 3600) / 60);
  const secs = s % 60;

  if (days > 0) {
    return isRu ? `${days}д ${hours}ч` : `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return isRu ? `${hours}ч ${mins}м` : `${hours}h ${mins}m`;
  }
  if (mins > 0) {
    return isRu ? `${mins}м ${secs}с` : `${mins}m ${secs}s`;
  }
  return isRu ? `${secs}с` : `${secs}s`;
}

export function formatRelativeAgo(iso: string | null | undefined, isRu: boolean): string {
  if (!iso) return '—';
  const ts = Date.parse(iso);
  if (!Number.isFinite(ts)) return '—';
  const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (diffSec < 10) return isRu ? 'только что' : 'just now';
  if (diffSec < 60) return isRu ? `${diffSec}с назад` : `${diffSec}s ago`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return isRu ? `${diffMin}м назад` : `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 48) return isRu ? `${diffHr}ч назад` : `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  return isRu ? `${diffDay}д назад` : `${diffDay}d ago`;
}

export function latencyBadgeClass(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) {
    return 'border-border/70 bg-muted/50 text-muted-foreground';
  }
  if (ms < 80) {
    return 'border-emerald-500/35 bg-emerald-500/12 text-emerald-400';
  }
  if (ms < 180) {
    return 'border-amber-500/35 bg-amber-500/12 text-amber-400';
  }
  return 'border-red-500/35 bg-red-500/12 text-red-400';
}
