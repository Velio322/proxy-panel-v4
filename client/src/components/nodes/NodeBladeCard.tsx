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
  HardDrive,
  MoreHorizontal,
  RefreshCw,
  Server,
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
  formatRelativeAgo,
  formatUptime,
  latencyBadgeClass,
} from '@/components/nodes/NodeTelemetryPrimitives';
import { cn } from '@/lib/core-utils';
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

function loadToneClass(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return 'text-muted-foreground';
  if (value >= 85) return 'text-red-500';
  if (value >= 65) return 'text-amber-500';
  return 'text-emerald-500';
}

export function NodeBladeCard({
  node,
  isRu,
  busyAction,
  onAction,
  onEdit,
  onOpenInstall,
}: Omit<NodeBladeCardProps, 'language'>) {
  const [copiedHost, setCopiedHost] = useState(false);

  const isOnline = node.status === 'ONLINE';
  const rawUptime = node.uptime ?? node.nodeMetrics?.[0]?.uptime ?? null;
  const hasUptime = isOnline && rawUptime != null && Number.isFinite(rawUptime);
  const cpu = node.cpuUsage ?? null;

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
      <div className={cn('absolute inset-y-0 left-0 w-1', theme.rail)} aria-hidden="true" />

      {/* Row 1: identity + quick action dock */}
      <div className="flex items-center gap-2 p-2 pl-3.5">
        <span className={cn('h-2 w-2 shrink-0 rounded-full', theme.dot)} aria-hidden="true" />
        <Link
          to={`/nodes/${node.id}`}
          className="min-w-0 flex-1 break-words text-sm font-bold tracking-tight transition-colors hover:text-primary"
        >
          {node.name}
        </Link>
        <Badge variant={theme.badgeVariant} className={cn('shrink-0 px-1.5 py-0 text-[10px]', theme.badgeClass)}>
          {node.status}
        </Badge>

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
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="icon" variant="ghost" className="h-7 w-7" aria-label={isRu ? 'Ещё действия' : 'More actions'}>
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-52">
              <DropdownMenuItem onClick={() => onEdit(node)}>
                <Edit className="h-3.5 w-3.5" />
                <span>{isRu ? 'Изменить параметры' : 'Edit Node'}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onOpenInstall(node)}>
                <Terminal className="h-3.5 w-3.5" />
                <span>{isRu ? 'Команда установки' : 'Install Command'}</span>
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

      {/* Row 2: compact vitals — ping, CPU, RAM, endpoint */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/40 px-2 py-1.5 pl-3.5 text-[11px]">
        <button
          type="button"
          onClick={handleCopyEndpoint}
          className="inline-flex min-w-0 max-w-[16rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={isRu ? 'Скопировать адрес' : 'Copy host:port'}
        >
          <Server className="h-3 w-3 shrink-0 text-primary/70" />
          <span className="break-all">
            {node.host}:{node.port}
          </span>
          {copiedHost ? (
            <Check className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
        </button>

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono font-medium tabular-nums',
            latencyBadgeClass(node.lastPingMs),
          )}
          title={isRu ? 'Пинг' : 'Ping'}
        >
          <Signal className="h-3 w-3 shrink-0" />
          {node.lastPingMs != null ? `${node.lastPingMs} ms` : '— ms'}
        </span>

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0.5 font-mono text-[11px] font-medium tabular-nums',
            loadToneClass(cpu),
          )}
          title="CPU"
        >
          <Cpu className="h-3 w-3 shrink-0" />
          {cpu != null ? `${cpu.toFixed(1)}%` : '—'}
        </span>

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0.5 font-mono text-[11px] font-medium tabular-nums',
            loadToneClass(node.memUsage),
          )}
          title="RAM"
        >
          <HardDrive className="h-3 w-3 shrink-0" />
          {node.memUsage != null ? `${node.memUsage.toFixed(1)}%` : '—'}
        </span>

        {hasUptime && (
          <span
            className="inline-flex items-center gap-1 rounded border border-emerald-500/35 bg-emerald-500/12 px-1.5 py-0.5 font-mono text-[11px] text-emerald-700 tabular-nums dark:text-emerald-300"
            title={isRu ? 'Аптайм сервера' : 'Node uptime'}
          >
            <Clock className="h-3 w-3 shrink-0" />
            {formatUptime(rawUptime, isRu)}
          </span>
        )}

        <span className="flex-1" />

        <span className="text-[11px] text-muted-foreground/90">
          {formatRelativeAgo(node.lastCheckAt, isRu)}
        </span>
        <Link
          to={`/nodes/${node.id}`}
          className="inline-flex items-center gap-0.5 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 text-[11px] font-medium text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={isRu ? 'Открыть ноду' : 'Open node'}
        >
          {isRu ? 'Консоль' : 'Console'}
          <ArrowRight className="h-3 w-3" />
        </Link>
      </div>
    </div>
  );
}
