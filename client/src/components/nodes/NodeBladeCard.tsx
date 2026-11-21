import { useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Activity,
  ArrowRight,
  Check,
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

function railColor(status: NodeStatus): string {
  switch (status) {
    case 'ONLINE':
      return 'bg-emerald-400 shadow-[0_0_12px_hsl(162_84%_43%/0.65)]';
    case 'MAINTENANCE':
      return 'bg-amber-400';
    case 'ERROR':
      return 'bg-red-500';
    default:
      return 'bg-zinc-600';
  }
}

function statusBadgeVariant(status: NodeStatus) {
  switch (status) {
    case 'ONLINE':
      return 'success' as const;
    case 'ERROR':
      return 'destructive' as const;
    case 'MAINTENANCE':
      return 'warning' as const;
    default:
      return 'secondary' as const;
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
      className={cn(
        'node-blade relative overflow-hidden rounded-xl border border-border/80 transition-colors hover:border-primary/45',
      )}
    >
      {/* Left vertical status rail */}
      <div className={cn('absolute inset-y-0 left-0 w-1', railColor(node.status))} aria-hidden="true" />

      <div className="space-y-3.5 p-4 pl-5 sm:p-5 sm:pl-6">
        {/* Header Row */}
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0 space-y-1.5">
            <div className="flex flex-wrap items-center gap-2">
              <span
                className={cn(
                  'h-2.5 w-2.5 shrink-0 rounded-full',
                  node.status === 'ONLINE'
                    ? 'bg-emerald-400 animate-pulse shadow-[0_0_8px_hsl(162_84%_43%/0.8)]'
                    : node.status === 'MAINTENANCE'
                      ? 'bg-amber-400'
                      : node.status === 'ERROR'
                        ? 'bg-red-500'
                        : 'bg-zinc-500',
                )}
              />
              <Link
                to={`/nodes/${node.id}`}
                className="text-base font-bold tracking-tight hover:text-primary transition-colors"
              >
                {node.name}
              </Link>
              <Badge variant={statusBadgeVariant(node.status)} className="text-[10px]">
                {node.status}
              </Badge>
              {node.arch && (
                <Badge variant="outline" className="font-mono text-[10px]">
                  {node.arch}
                </Badge>
              )}
              {node.version && (
                <Badge variant="secondary" className="font-mono text-[10px]">
                  agent v{node.version.replace(/^v/i, '')}
                </Badge>
              )}
              {node.mtlsFingerprint && (
                <Badge variant="success" className="text-[10px]">
                  <ShieldCheck className="h-3 w-3" /> mTLS
                </Badge>
              )}
              {locationParts.length > 0 && (
                <span className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                  <Globe className="h-3 w-3 text-primary/80" />
                  {locationParts.join(' · ')}
                </span>
              )}
            </div>

            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <button
                type="button"
                onClick={handleCopyEndpoint}
                className="inline-flex items-center gap-1.5 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 font-mono text-xs text-foreground transition-colors hover:border-primary/45 hover:text-primary"
                title={isRu ? 'Скопировать адрес' : 'Copy host:port'}
              >
                <span>
                  {node.host}:{node.port}
                </span>
                {copiedHost ? (
                  <Check className="h-3 w-3 text-emerald-400" />
                ) : (
                  <Copy className="h-3 w-3 text-muted-foreground" />
                )}
              </button>
              <span className="font-mono text-[11px]">API :{node.apiPort}</span>
              <span className="text-border">·</span>
              <span className="text-[11px]">
                {isRu ? 'Обновлено:' : 'Checked:'} {formatRelativeAgo(node.lastCheckAt, isRu)}
              </span>
            </div>
          </div>

          {/* Right Live Latency + Quick Control Dock */}
          <div className="flex flex-wrap items-center gap-1.5">
            <span
              className={cn(
                'inline-flex items-center gap-1 rounded-md border px-2 py-1 font-mono text-xs font-medium tabular-nums',
                latencyBadgeClass(node.lastPingMs),
              )}
            >
              <Signal className="h-3 w-3" />
              {node.lastPingMs != null ? `${node.lastPingMs} ms` : '— ms'}
            </span>

            <Button
              size="sm"
              variant="outline"
              className="h-8 gap-1 px-2.5 text-xs"
              loading={busyAction === 'check'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'check')}
            >
              <Activity className="h-3.5 w-3.5 text-primary" />
              <span className="hidden sm:inline">{isRu ? 'Проверить' : 'Check'}</span>
            </Button>

            <Button
              size="sm"
              variant="outline"
              className="h-8 gap-1 px-2.5 text-xs"
              loading={busyAction === 'push'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'push')}
            >
              <UploadCloud className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">{isRu ? 'Синхр.' : 'Push'}</span>
            </Button>

            <Button
              size="sm"
              variant="outline"
              className="h-8 gap-1 px-2.5 text-xs"
              loading={busyAction === 'restart'}
              disabled={Boolean(busyAction)}
              onClick={() => onAction(node.id, 'restart')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              <span className="hidden md:inline">{isRu ? 'Рестарт' : 'Restart'}</span>
            </Button>

            <Button
              size="sm"
              variant="ghost"
              className="h-8 w-8 p-0"
              onClick={() => onEdit(node)}
              aria-label={isRu ? 'Редактировать ноду' : 'Edit node'}
              title={isRu ? 'Редактировать' : 'Edit'}
            >
              <Edit className="h-3.5 w-3.5" />
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="More actions">
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

        {/* Middle 3-Column Telemetry Strip */}
        <div className="grid gap-3 md:grid-cols-3">
          {/* CPU Box */}
          <div className="rounded-lg border border-border/70 bg-background/55 p-3 space-y-2">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <Cpu className="h-3.5 w-3.5 text-emerald-400" />
                <span>CPU</span>
              </div>
              <span className="font-mono text-sm font-bold tabular-nums">
                {node.cpuUsage != null ? `${node.cpuUsage.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={node.cpuUsage} tone="emerald" />
            <div className="flex items-center justify-between pt-0.5">
              <Sparkline values={cpuSeries} width={150} height={24} stroke="#10b981" id={`cpu-${node.id}`} />
              <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
                {cpuSeries.length > 0 ? `${cpuSeries.length} pts` : isRu ? 'нет истории' : 'no samples'}
              </span>
            </div>
          </div>

          {/* RAM Box */}
          <div className="rounded-lg border border-border/70 bg-background/55 p-3 space-y-2">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <HardDrive className="h-3.5 w-3.5 text-cyan-400" />
                <span>RAM</span>
              </div>
              <span className="font-mono text-sm font-bold tabular-nums">
                {node.memUsage != null ? `${node.memUsage.toFixed(1)}%` : '—'}
              </span>
            </div>
            <MeterBar value={node.memUsage} tone="cyan" />
            <div className="flex items-center justify-between pt-0.5">
              <Sparkline values={memSeries} width={150} height={24} stroke="#06b6d4" id={`ram-${node.id}`} />
              <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
                {memSeries.length > 0 ? `${memSeries.length} pts` : isRu ? 'нет истории' : 'no samples'}
              </span>
            </div>
          </div>

          {/* Traffic & Network Box */}
          <div className="rounded-lg border border-border/70 bg-background/55 p-3 space-y-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-muted-foreground">
                {isRu ? 'Трафик ноды' : 'Node Traffic'}
              </span>
              <span className="font-mono text-xs font-semibold tabular-nums text-foreground">
                {formatBytes(totalTraffic, { locale: language })}
              </span>
            </div>

            <div className="flex items-center justify-between font-mono text-xs tabular-nums">
              <span className="text-emerald-400">↓ {formatBytes(totalDl, { locale: language })}</span>
              <span className="text-cyan-400">↑ {formatBytes(totalUl, { locale: language })}</span>
            </div>

            <div className="telemetry-track flex h-1.5 w-full overflow-hidden rounded-full">
              <div className="h-full bg-emerald-400" style={{ width: `${dlSharePct}%` }} />
              <div className="h-full bg-cyan-400" style={{ width: `${100 - dlSharePct}%` }} />
            </div>

            <div className="flex items-center justify-between font-mono text-[10px] text-muted-foreground tabular-nums">
              {latestMetric?.uploadBps != null || latestMetric?.downloadBps != null ? (
                <span>
                  ↑ {formatBps(latestMetric.uploadBps, language)} · ↓{' '}
                  {formatBps(latestMetric.downloadBps, language)}
                </span>
              ) : (
                <span>{isRu ? 'Счётчик туннелей' : 'Tunnel counter'}</span>
              )}
              {(latestMetric?.tcpConnections != null || latestMetric?.udpSockets != null) && (
                <span>
                  TCP {latestMetric.tcpConnections ?? 0} · UDP {latestMetric.udpSockets ?? 0}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Bottom 2-Column Endpoints & Multi-Core Engine Matrix */}
        <div className="grid gap-3 md:grid-cols-2">
          {/* Active Protocol Endpoints */}
          <div className="rounded-lg border border-border/60 bg-background/35 p-3 space-y-2">
            <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground">
              <span>{isRu ? `Протоколы и порты (${inbounds.length})` : `Endpoints (${inbounds.length})`}</span>
              <Link to="/inbounds" className="text-primary hover:underline">
                {isRu ? 'Все протоколы' : 'Manage'}
              </Link>
            </div>
            {inbounds.length === 0 ? (
              <div className="flex items-center justify-between py-1 text-xs text-muted-foreground">
                <span>{isRu ? 'Нет настроенных протоколов' : 'No inbounds configured'}</span>
                <Button size="sm" variant="outline" asChild className="h-7 px-2 text-xs">
                  <Link to="/inbounds?create=1">
                    <Plus className="h-3 w-3" /> {isRu ? 'Добавить' : 'Add'}
                  </Link>
                </Button>
              </div>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {inbounds.map((ib) => (
                  <Link
                    key={ib.id}
                    to="/inbounds"
                    className={cn(
                      'inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-opacity hover:opacity-90',
                      PROTOCOL_COLORS[ib.protocol],
                      !ib.enable && 'opacity-50',
                    )}
                    title={ib.tag}
                  >
                    <span
                      className={cn(
                        'h-1.5 w-1.5 rounded-full',
                        ib.enable ? 'bg-emerald-400' : 'bg-zinc-500',
                      )}
                    />
                    <span>{ib.protocol}</span>
                    <span className="font-mono font-semibold">:{ib.port}</span>
                    <span className="hidden xl:inline max-w-[120px] truncate opacity-75">
                      {ib.tag}
                    </span>
                  </Link>
                ))}
              </div>
            )}
          </div>

          {/* 4-Core Engine Versions Matrix */}
          <div className="rounded-lg border border-border/60 bg-background/35 p-3 space-y-2">
            <div className="flex items-center justify-between text-[11px] font-medium text-muted-foreground">
              <span>{isRu ? 'Ядра прокси на ноде' : 'Installed Core Engines'}</span>
              <Link to={`/nodes/${node.id}?tab=updates`} className="text-primary hover:underline">
                {isRu ? 'Обновить ядра' : 'Update cores'}
              </Link>
            </div>
            <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
              {coreChips.map((c) => (
                <Link
                  key={c.key}
                  to={`/nodes/${node.id}?tab=updates`}
                  className="flex flex-col rounded-md border border-border/70 bg-background/70 px-2 py-1.5 transition-colors hover:border-primary/45"
                >
                  <span className="text-[10px] text-muted-foreground">{c.key}</span>
                  <span className="font-mono text-xs font-semibold truncate">
                    {c.version || '—'}
                  </span>
                </Link>
              ))}
            </div>
          </div>
        </div>

        {/* Footer Strip */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2.5 text-xs">
          <div className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
            <Server className="h-3.5 w-3.5 text-primary/70" />
            <span className="font-mono text-[11px]">{node.id.slice(0, 8)}</span>
            {(node.tags ?? []).map((tag) => (
              <Badge key={tag} variant="outline" className="text-[10px]">
                #{tag}
              </Badge>
            ))}
          </div>

          <Button size="sm" variant="secondary" asChild className="h-8 gap-1.5 px-3 text-xs font-medium">
            <Link to={`/nodes/${node.id}`}>
              <span>{isRu ? 'Консоль ноды' : 'Open Console'}</span>
              <ArrowRight className="h-3.5 w-3.5" />
            </Link>
          </Button>
        </div>
      </div>
    </div>
  );
}
