import { useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Activity,
  ArrowRight,
  Check,
  Clock,
  Copy,
  Cpu,
  Edit,
  Globe,
  HardDrive,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Server,
  ShieldCheck,
  Signal,
  Terminal,
  Trash2,
  UploadCloud,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  MeterBar,
  Sparkline,
  formatRelativeAgo,
  formatUptime,
  latencyBadgeClass,
} from '@/components/nodes/NodeTelemetryPrimitives';
import { PROTOCOL_COLORS, cn, formatBps, formatBytes } from '@/lib/core-utils';
import type { Language, NodeItem, NodeStatus } from '@/types';

interface NodeBladeCardProps {
  node: NodeItem;
  isRu: boolean;
  language: Language;
  busyAction: string | null;
  onAction: (id: string, action: 'check' | 'push' | 'restart' | 'delete') => void;
  onEdit: (node: NodeItem) => void;
  onOpenInstall: (node: NodeItem) => void;
}

function statusTheme(status: NodeStatus) {
  switch (status) {
    case 'ONLINE':
      return {
        surface: 'node-blade-online border-emerald-500/40 hover:border-emerald-400/65',
        rail: 'bg-emerald-400 shadow-[0_0_12px_hsl(162_84%_43%/0.65)]',
        dot: 'bg-emerald-400 animate-pulse shadow-[0_0_8px_hsl(162_84%_43%/0.8)]',
        badgeVariant: 'success' as const,
        badgeClass:
          'border-emerald-500/45 bg-emerald-500/20 text-emerald-700 dark:text-emerald-300 font-semibold',
      };
    case 'MAINTENANCE':
      return {
        surface: 'node-blade-maintenance border-amber-500/45 hover:border-amber-400/70',
        rail: 'bg-amber-400 shadow-[0_0_10px_rgba(251,191,36,0.45)]',
        dot: 'bg-amber-400',
        badgeVariant: 'warning' as const,
        badgeClass:
          'border-amber-500/45 bg-amber-500/20 text-amber-700 dark:text-amber-300 font-semibold',
      };
    default:
      return {
        surface: 'node-blade-offline border-red-500/50 hover:border-red-400/75',
        rail: 'bg-red-500 shadow-[0_0_12px_rgba(239,68,68,0.6)]',
        dot: 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.7)]',
        badgeVariant: 'destructive' as const,
        badgeClass:
          'border-red-500/50 bg-red-500/20 text-red-700 dark:text-red-300 font-semibold',
      };
  }
}

export function NodeBladeCard({
  node,
  isRu,
  language,
  busyAction,
  onAction,
  onEdit,
  onOpenInstall,
}: NodeBladeCardProps) {
  const [copiedHost, setCopiedHost] = useState(false);

  const chronoMetrics = [...(node.nodeMetrics ?? [])].reverse();
  const cpuSeries = chronoMetrics
    .map((m) => m.cpuUsage)
    .filter((v): v is number => v !== null && v !== undefined && Number.isFinite(v));
  const memSeries = chronoMetrics
    .map((m) => m.memUsage)
    .filter((v): v is number => v !== null && v !== undefined && Number.isFinite(v));
  const latestMetric = node.nodeMetrics?.[0];

  const rawUptime = node.uptime ?? latestMetric?.uptime ?? null;
  const isOnline = node.status === 'ONLINE';
  const hasUptime = isOnline && rawUptime != null && Number.isFinite(rawUptime);
  const uptimeDisplay = hasUptime ? `↑ ${formatUptime(rawUptime, isRu)}` : '—';

  const totalDl = node.netDownload ?? 0;
  const totalUl = node.netUpload ?? 0;
  const totalTraffic = totalDl + totalUl;
  const dlSharePct = totalTraffic > 0 ? Math.round((totalDl / totalTraffic) * 100) : 50;

  const locationParts = [node.country, node.city, node.isp].filter(
    (s): s is string => Boolean(s && s.trim()),
  );
  const inbounds = [...(node.inbounds ?? [])].sort((a, b) => a.port - b.port);

  const coreChips = [
    { key: 'Xray', version: node.xrayVersion },
    { key: 'Sing-box', version: node.singboxVersion },
    { key: 'Naive', version: node.naiveVersion },
    { key: 'Mieru', version: node.mieruVersion },
  ];

  const theme = statusTheme(node.status);

  const handleCopyEndpoint = async () => {
    try {
      await navigator.clipboard.writeText(`${node.host}:${node.port}`);
      setCopiedHost(true);
      window.setTimeout(() => setCopiedHost(false), 1400);
    } catch {
      // clipboard permission optional
    }
  };

  return (
    <div
      data-node-status={node.status}
      className={cn(
        'node-blade relative flex flex-col overflow-hidden rounded-xl border transition-colors',
        theme.surface,
      )}
    >
      {/* Left vertical status rail */}
      <div className={cn('absolute inset-y-0 left-0 w-1', theme.rail)} aria-hidden="true" />

      <div className="flex flex-1 flex-col space-y-2.5 p-3.5 pl-4 sm:p-4 sm:pl-5">
        {/* Header Row 1: Identity + Quick Icon Dock */}
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className={cn('h-2 w-2 shrink-0 rounded-full', theme.dot)} />
              <Link
                to={`/nodes/${node.id}`}
                className="break-words text-sm sm:text-base font-bold tracking-tight hover:text-primary transition-colors min-w-0"
              >
                {node.name}
              </Link>
              <Badge variant={theme.badgeVariant} className={cn('px-1.5 py-0 text-[10px]', theme.badgeClass)}>
                {node.status}
              </Badge>
              {node.arch && (
                <Badge variant="outline" className="px-1.5 py-0 font-mono text-[10px]">
                  {node.arch}
                </Badge>
              )}
              {node.version && (
                <Badge variant="secondary" className="px-1.5 py-0 font-mono text-[10px]">
                  v{node.version.replace(/^v/i, '')}
                </Badge>
              )}
              {node.mtlsFingerprint && (
                <Badge variant="success" className="px-1.5 py-0 text-[10px]">
                  <ShieldCheck className="h-2.5 w-2.5" /> mTLS
                </Badge>
              )}
              {locationParts.length > 0 && (
                <span className="inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0 text-[10px] text-muted-foreground">
                  <Globe className="h-2.5 w-2.5 text-primary/80" />
                  <span className="break-words min-w-0">{locationParts.join(' · ')}</span>
                </span>
              )}
            </div>

            {/* Header Row 2: Endpoint, API, Ping, Uptime, Checked */}
            <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
              <button
                type="button"
                onClick={handleCopyEndpoint}
                className="inline-flex items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
                title={isRu ? 'Скопировать адрес' : 'Copy host:port'}
              >
                <span className="break-words min-w-0">
                  {node.host}:{node.port}
                </span>
                {copiedHost ? (
                  <Check className="h-3 w-3 shrink-0 text-emerald-400" />
                ) : (
                  <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
                )}
              </button>

              <span className="font-mono text-[11px]">API :{node.apiPort}</span>

              <span
                className={cn(
                  'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[11px] font-medium tabular-nums',
                  latencyBadgeClass(node.lastPingMs),
                )}
              >
                <Signal className="h-3 w-3 shrink-0" />
                {node.lastPingMs != null ? `${node.lastPingMs} ms` : '— ms'}
              </span>

              <span
                className={cn(
                  'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[11px] font-medium tabular-nums',
                  hasUptime
                    ? 'border-emerald-500/35 bg-emerald-500/12 text-emerald-700 dark:text-emerald-300'
                    : 'border-border/70 bg-muted/40 text-muted-foreground',
                )}
                title={
                  isOnline
                    ? isRu
                      ? 'Аптайм сервера'
                      : 'Node uptime'
                    : isRu
                      ? `Был в сети: ${formatRelativeAgo(node.lastCheckAt, isRu)}`
                      : `Last seen: ${formatRelativeAgo(node.lastCheckAt, isRu)}`
                }
              >
                <Clock className="h-3 w-3 shrink-0" />
                <span>{uptimeDisplay}</span>
              </span>

              <span className="text-[11px] text-muted-foreground/90">
                {isRu ? 'Обновл.:' : 'Checked:'} {formatRelativeAgo(node.lastCheckAt, isRu)}
              </span>
            </div>
          </div>

          {/* Right Quick Action Dock */}
          <div className="flex shrink-0 items-center gap-1">
            <Button
              size="icon"
              variant="outline"
              className="h-7 w-7"
              loading={busyAction === 'check'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'check')}
              aria-label={isRu ? 'Проверить ноду' : 'Check node'}
              title={isRu ? 'Проверить' : 'Check'}
            >
              <Activity className="h-3.5 w-3.5 text-primary" />
            </Button>

            <Button
              size="icon"
              variant="outline"
              className="h-7 w-7"
              loading={busyAction === 'push'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'push')}
              aria-label={isRu ? 'Синхронизировать конфиг' : 'Push config'}
              title={isRu ? 'Синхронизировать конфиг' : 'Push config'}
            >
              <UploadCloud className="h-3.5 w-3.5" />
            </Button>

            <Button
              size="icon"
              variant="outline"
              className="h-7 w-7"
              loading={busyAction === 'restart'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'restart')}
              aria-label={isRu ? 'Рестарт ядер' : 'Restart cores'}
              title={isRu ? 'Рестарт ядер' : 'Restart cores'}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>

            <Button
              size="icon"
              variant="ghost"
              className="h-7 w-7"
              onClick={() => onEdit(node)}
              aria-label={isRu ? 'Редактировать ноду' : 'Edit node'}
              title={isRu ? 'Редактировать' : 'Edit'}
            >
              <Edit className="h-3.5 w-3.5" />
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button size="icon" variant="ghost" className="h-7 w-7" aria-label="More actions">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-52">
                <DropdownMenuItem onClick={() => onOpenInstall(node)}>
                  <Terminal className="h-3.5 w-3.5" />
                  <span>{isRu ? 'Команда установки' : 'Install Command'}</span>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onEdit(node)}>
                  <Edit className="h-3.5 w-3.5" />
                  <span>{isRu ? 'Изменить параметры' : 'Edit Node'}</span>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => onAction(node.id, 'delete')}
                  className="text-red-500 dark:text-red-400 focus:text-red-500 dark:focus:text-red-400"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  <span>{isRu ? 'Удалить ноду' : 'Delete Node'}</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        {/* Middle 3-Column Compact Telemetry Strip */}
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {/* CPU Box */}
          <div className="min-w-0 rounded-lg border border-border/70 bg-background/55 p-2.5 space-y-1.5">
            <div className="flex items-center justify-between gap-1.5">
              <div className="flex items-center gap-1 text-[11px] text-muted-foreground">
                <Cpu className="h-3 w-3 text-emerald-400" />
                <span>CPU</span>
              </div>
              <span className="font-mono text-xs font-bold tabular-nums">
                {node.cpuUsage != null ? `${node.cpuUsage.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={node.cpuUsage} tone="emerald" />
            <div className="flex items-center justify-between gap-1 pt-0.5">
              <Sparkline
                values={cpuSeries}
                width={104}
                height={20}
                stroke="#10b981"
                id={`cpu-${node.id}`}
                className="min-w-0 shrink"
              />
              <span className="shrink-0 font-mono text-[10px] text-muted-foreground tabular-nums">
                {cpuSeries.length > 0 ? `${cpuSeries.length} pts` : isRu ? 'нет' : '0 pts'}
              </span>
            </div>
          </div>

          {/* RAM Box */}
          <div className="min-w-0 rounded-lg border border-border/70 bg-background/55 p-2.5 space-y-1.5">
            <div className="flex items-center justify-between gap-1.5">
              <div className="flex items-center gap-1 text-[11px] text-muted-foreground">
                <HardDrive className="h-3 w-3 text-cyan-400" />
                <span>RAM</span>
              </div>
              <span className="font-mono text-xs font-bold tabular-nums">
                {node.memUsage != null ? `${node.memUsage.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={node.memUsage} tone="cyan" />
            <div className="flex items-center justify-between gap-1 pt-0.5">
              <Sparkline
                values={memSeries}
                width={104}
                height={20}
                stroke="#06b6d4"
                id={`ram-${node.id}`}
                className="min-w-0 shrink"
              />
              <span className="shrink-0 font-mono text-[10px] text-muted-foreground tabular-nums">
                {memSeries.length > 0 ? `${memSeries.length} pts` : isRu ? 'нет' : '0 pts'}
              </span>
            </div>
          </div>

          {/* Traffic & Network Box */}
          <div className="min-w-0 rounded-lg border border-border/70 bg-background/55 p-2.5 space-y-1.5">
            <div className="flex items-center justify-between gap-1.5">
              <span className="text-[11px] text-muted-foreground break-words min-w-0">
                {isRu ? 'Трафик' : 'Traffic'}
              </span>
              <span className="font-mono text-xs font-bold tabular-nums text-foreground">
                {formatBytes(totalTraffic, { locale: language })}
              </span>
            </div>

            <div className="flex items-center justify-between gap-1 font-mono text-[11px] tabular-nums">
              <span className="break-words text-emerald-400 min-w-0">↓ {formatBytes(totalDl, { locale: language })}</span>
              <span className="break-words text-cyan-400 min-w-0">↑ {formatBytes(totalUl, { locale: language })}</span>
            </div>

            <div className="telemetry-track flex h-1.5 w-full overflow-hidden rounded-full">
              <div className="h-full bg-emerald-400" style={{ width: `${dlSharePct}%` }} />
              <div className="h-full bg-cyan-400" style={{ width: `${100 - dlSharePct}%` }} />
            </div>

            <div className="flex items-center justify-between gap-1 font-mono text-[10px] text-muted-foreground tabular-nums">
              {latestMetric?.uploadBps != null || latestMetric?.downloadBps != null ? (
                <span className="break-words min-w-0">
                  ↑{formatBps(latestMetric.uploadBps, language)} · ↓
                  {formatBps(latestMetric.downloadBps, language)}
                </span>
              ) : (
                <span className="break-words min-w-0">{isRu ? 'Счётчик' : 'Counter'}</span>
              )}
              {(latestMetric?.tcpConnections != null || latestMetric?.udpSockets != null) && (
                <span className="shrink-0">
                  T{latestMetric.tcpConnections ?? 0}/U{latestMetric.udpSockets ?? 0}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Bottom 2-Column Endpoints & Multi-Core Engine Matrix */}
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {/* Active Protocol Endpoints */}
          <div className="min-w-0 rounded-lg border border-border/60 bg-background/35 p-2.5 space-y-1.5">
            <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground">
              <span>{isRu ? `Протоколы (${inbounds.length})` : `Endpoints (${inbounds.length})`}</span>
              <Link to="/inbounds" className="text-primary hover:underline">
                {isRu ? 'Все' : 'Manage'}
              </Link>
            </div>
            {inbounds.length === 0 ? (
              <div className="flex items-center justify-between py-0.5 text-xs text-muted-foreground">
                <span>{isRu ? 'Нет протоколов' : 'No inbounds'}</span>
                <Button size="sm" variant="outline" asChild className="h-6 px-2 text-[11px]">
                  <Link to="/inbounds?create=1">
                    <Plus className="h-3 w-3" /> {isRu ? 'Добавить' : 'Add'}
                  </Link>
                </Button>
              </div>
            ) : (
              <div className="flex flex-wrap gap-1">
                {inbounds.map((ib) => (
                  <Link
                    key={ib.id}
                    to="/inbounds"
                    className={cn(
                      'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] font-medium transition-opacity hover:opacity-90',
                      PROTOCOL_COLORS[ib.protocol],
                      !ib.enable && 'opacity-50',
                    )}
                    title={`${ib.protocol}:${ib.port} · ${ib.tag}`}
                  >
                    <span
                      className={cn(
                        'h-1.5 w-1.5 rounded-full',
                        ib.enable ? 'bg-emerald-400' : 'bg-zinc-500',
                      )}
                    />
                    <span>{ib.protocol}</span>
                    <span className="font-mono font-semibold">:{ib.port}</span>
                  </Link>
                ))}
              </div>
            )}
          </div>

          {/* 4-Core Engine Versions 2x2 Matrix */}
          <div className="min-w-0 rounded-lg border border-border/60 bg-background/35 p-2.5 space-y-1.5">
            <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground">
              <span>{isRu ? 'Ядра прокси' : 'Core Engines'}</span>
              <Link to={`/nodes/${node.id}?tab=updates`} className="text-primary hover:underline">
                {isRu ? 'Обновить' : 'Update'}
              </Link>
            </div>
            <div className="grid grid-cols-2 gap-1">
              {coreChips.map((c) => (
                <Link
                  key={c.key}
                  to={`/nodes/${node.id}?tab=updates`}
                  className="flex items-center justify-between gap-1 rounded border border-border/70 bg-background/70 px-1.5 py-1 transition-colors hover:border-primary/45"
                >
                  <span className="text-[10px] text-muted-foreground break-words min-w-0">{c.key}</span>
                  <span className="font-mono text-[11px] font-semibold break-words min-w-0">
                    {c.version || '—'}
                  </span>
                </Link>
              ))}
            </div>
          </div>
        </div>

        {/* Footer Strip */}
        <div className="mt-auto flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2 text-xs">
          <div className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
            <Server className="h-3.5 w-3.5 text-primary/70" />
            <span className="font-mono text-[11px]">{node.id.slice(0, 8)}</span>
            {(node.tags ?? []).map((tag) => (
              <Badge key={tag} variant="outline" className="px-1.5 py-0 text-[10px]">
                #{tag}
              </Badge>
            ))}
          </div>

          <Button size="sm" variant="secondary" asChild className="h-7 gap-1 px-2.5 text-xs font-medium">
            <Link to={`/nodes/${node.id}`}>
              <span>{isRu ? 'Консоль' : 'Console'}</span>
              <ArrowRight className="h-3 w-3" />
            </Link>
          </Button>
        </div>
      </div>
    </div>
  );
}
