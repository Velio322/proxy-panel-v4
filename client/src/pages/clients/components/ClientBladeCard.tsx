import { Link } from 'react-router-dom';
import {
  Activity,
  Ban,
  Calendar,
  Check,
  Copy,
  Edit,
  MoreHorizontal,
  QrCode,
  RotateCcw,
  Server,
  ShieldCheck,
  Smartphone,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MeterBar, formatRelativeAgo } from '@/components/nodes/NodeTelemetryPrimitives';
import { PROTOCOL_COLORS, cn, formatBytes, formatDate } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import type { Client } from '@/types';

interface ClientBladeCardProps {
  client: Client;
  selected: boolean;
  copiedUuid: boolean;
  onToggleSelect: () => void;
  onCopyUuid: () => void;
  onOpenSubscription: () => void;
  onEdit: () => void;
  onEditNodes: () => void;
  onResetTraffic: () => void;
  onToggleBan: () => void;
  onDelete: () => void;
}

export function ClientBladeCard({
  client,
  selected,
  copiedUuid,
  onToggleSelect,
  onCopyUuid,
  onOpenSubscription,
  onEdit,
  onEditNodes,
  onResetTraffic,
  onToggleBan,
  onDelete,
}: ClientBladeCardProps) {
  const { t } = useTranslation();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const used = Number(client.usedTraffic || 0);
  const limit = Number(client.trafficLimit || 0);
  const ul = Number(client.uploadTraffic || 0);
  const dl = Number(client.downloadTraffic || 0);
  const pct = limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;

  const railClass = client.banned
    ? 'bg-red-500'
    : client.onHold
      ? 'bg-amber-400'
      : 'bg-emerald-400 shadow-[0_0_12px_hsl(162_84%_43%/0.6)]';

  const expireTs = client.expireAt ? Date.parse(client.expireAt) : null;
  const isExpired = expireTs != null && Number.isFinite(expireTs) && expireTs < Date.now();
  const isExpiringSoon =
    expireTs != null &&
    Number.isFinite(expireTs) &&
    !isExpired &&
    expireTs - Date.now() < 3 * 86400 * 1000;

  return (
    <div
      className={cn(
        'node-blade relative overflow-hidden rounded-xl border border-border/80 transition-colors hover:border-primary/45',
        selected && 'border-primary/60 ring-1 ring-primary/30',
      )}
    >
      <div className={cn('absolute inset-y-0 left-0 w-1', railClass)} aria-hidden="true" />

      <div className="space-y-2 p-3 pl-4">
        {/* Header Row */}
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex items-start gap-3 min-w-0">
            <div className="pt-1">
              <Checkbox
                checked={selected}
                onCheckedChange={onToggleSelect}
                aria-label={`Select ${client.username}`}
              />
            </div>
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-primary/25 bg-primary/12 font-mono text-xs font-bold uppercase text-primary">
              {client.username.slice(0, 2)}
            </div>
            <div className="min-w-0 space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={onEdit}
                  className="text-base font-bold tracking-tight hover:text-primary transition-colors text-left"
                >
                  {client.username}
                </button>
                {client.banned ? (
                  <Badge variant="destructive" className="text-[10px]">
                    {t('common.banned')}
                  </Badge>
                ) : client.onHold ? (
                  <Badge variant="warning" className="text-[10px]">
                    {t('common.onHold')}
                  </Badge>
                ) : (
                  <Badge variant="success" className="text-[10px]">
                    {t('common.active')}
                  </Badge>
                )}
                {client.plan?.name && (
                  <Badge variant="secondary" className="text-[10px]">
                    {client.plan.name}
                  </Badge>
                )}
              </div>

              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <button
                  type="button"
                  onClick={onCopyUuid}
                  aria-label={`Copy UUID for ${client.username}`}
                  className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
                >
                  <span>{client.uuid.slice(0, 8)}…</span>
                  {copiedUuid ? (
                    <Check className="h-3 w-3 text-emerald-400" />
                  ) : (
                    <Copy className="h-3 w-3 text-muted-foreground" />
                  )}
                </button>
                {client.email && <span className="break-words min-w-0">{client.email}</span>}
                {client.note && (
                  <span className="break-words text-[11px] italic opacity-80 min-w-0">
                    “{client.note}”
                  </span>
                )}
              </div>
            </div>
          </div>

          {/* Right Quick Actions */}
          <div className="flex flex-wrap items-center gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="secondary"
              className="h-8 gap-1.5 px-2.5 text-xs border border-primary/25 bg-primary/10 text-primary hover:bg-primary/20"
              onClick={onOpenSubscription}
            >
              <QrCode className="h-3.5 w-3.5" />
              <span>{t('clients.keysQr')}</span>
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-8 gap-1 px-2.5 text-xs"
              onClick={onEditNodes}
            >
              <Server className="h-3.5 w-3.5 text-primary" />
              <span className="hidden sm:inline">{isRu ? 'Ноды' : 'Nodes'}</span>
            </Button>
            <Button size="sm" variant="outline" asChild className="h-8 gap-1 px-2.5 text-xs">
              <Link to={`/clients/${client.id}/traffic`}>
                <Activity className="h-3.5 w-3.5" />
                <span className="hidden sm:inline">{isRu ? 'Трафик' : 'Traffic'}</span>
              </Link>
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-8 gap-1 px-2.5 text-xs"
              onClick={onEdit}
            >
              <Edit className="h-3.5 w-3.5" />
              <span className="hidden md:inline">{t('common.edit')}</span>
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8"
                  aria-label={t('common.actions')}
                >
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuItem onClick={onEditNodes}>
                  <Server className="h-3.5 w-3.5 text-primary" />
                  <span>{t('clients.bindNodesAction')}</span>
                </DropdownMenuItem>
                <DropdownMenuItem asChild>
                  <Link to={`/clients/${client.id}/traffic`}>
                    <Activity className="h-3.5 w-3.5" />
                    <span>{t('clients.trafficChart')}</span>
                  </Link>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={onResetTraffic}>
                  <RotateCcw className="h-3.5 w-3.5" />
                  <span>{t('clients.resetTraffic')}</span>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={onToggleBan}>
                  {client.banned ? (
                    <>
                      <ShieldCheck className="h-3.5 w-3.5 text-emerald-400" />
                      <span>{t('clients.unban')}</span>
                    </>
                  ) : (
                    <>
                      <Ban className="h-3.5 w-3.5 text-amber-400" />
                      <span>{t('clients.ban')}</span>
                    </>
                  )}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={onDelete}
                  className="text-red-500 dark:text-red-400 focus:text-red-500 dark:focus:text-red-400"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  <span>{t('clients.deleteClient')}</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        {/* Middle 3-Column Telemetry Strip */}
        <div className="grid gap-2 md:grid-cols-3">
          {/* 1. Traffic Quota */}
          <div className="rack-inset rounded-lg p-2 space-y-2">
            <div className="flex items-center justify-between text-xs">
              <span className="text-muted-foreground">{t('clients.colTraffic')}</span>
              <span className="font-mono font-bold tabular-nums">
                {formatBytes(used, { locale: language })} /{' '}
                {limit > 0 ? formatBytes(limit, { locale: language }) : '∞'}
              </span>
            </div>
            <MeterBar value={limit > 0 ? pct : 35} tone={limit > 0 ? 'emerald' : 'cyan'} />
            <div className="flex items-center justify-between font-mono text-[11px] text-muted-foreground tabular-nums">
              <span>
                <span className="text-emerald-400">↓ {formatBytes(dl, { compact: true, locale: language })}</span>
                {' · '}
                <span className="text-cyan-400">↑ {formatBytes(ul, { compact: true, locale: language })}</span>
              </span>
              <span>{limit > 0 ? `${pct}%` : isRu ? 'Безлимит' : 'Unlimited'}</span>
            </div>
          </div>

          {/* 2. Protocols & Node Pinning */}
          <div className="rack-inset rounded-lg p-2 space-y-2">
            <div className="flex items-center justify-between text-xs">
              <span className="text-muted-foreground">{t('clients.colProtocols')}</span>
              <button
                type="button"
                onClick={onEditNodes}
                className="font-medium text-primary hover:underline text-[11px]"
              >
                {t('clients.bindNodesAction')}
              </button>
            </div>
            <div className="flex flex-wrap gap-1">
              {(client.protocols ?? []).map((proto) => (
                <Badge key={proto} className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[proto])}>
                  {proto}
                </Badge>
              ))}
            </div>
            <div className="flex flex-wrap items-center gap-1 pt-0.5">
              <Server className="h-3 w-3 text-muted-foreground shrink-0" />
              {client.allNodes !== false ? (
                <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground">
                  {t('clients.allNodesBadge')}
                </Badge>
              ) : (client.nodes?.length ?? 0) === 0 ? (
                <Badge variant="warning" className="text-[10px]">
                  {t('clients.noNodesBadge')}
                </Badge>
              ) : (
                <>
                  {(client.nodes ?? []).slice(0, 3).map((n) => (
                    <Badge key={n.id} variant="secondary" className="text-[10px] break-words min-w-0">
                      {n.name}
                    </Badge>
                  ))}
                  {(client.nodes?.length ?? 0) > 3 && (
                    <Badge variant="outline" className="text-[10px]">
                      +{(client.nodes?.length ?? 0) - 3}
                    </Badge>
                  )}
                </>
              )}
            </div>
          </div>

          {/* 3. Lifecycle & Devices */}
          <div className="rack-inset rounded-lg p-2 space-y-2 text-xs">
            <div className="flex items-center justify-between">
              <span className="inline-flex items-center gap-1 text-muted-foreground">
                <Calendar className="h-3 w-3" />
                <span>{t('clients.colExpires')}</span>
              </span>
              <span
                className={cn(
                  'font-mono font-semibold tabular-nums',
                  isExpired ? 'text-red-400' : isExpiringSoon ? 'text-amber-400' : 'text-foreground',
                )}
              >
                {client.expireAt ? formatDate(client.expireAt, undefined, timezone) : t('common.never')}
              </span>
            </div>
            <div className="flex items-center justify-between text-[11px] text-muted-foreground">
              <span className="inline-flex items-center gap-1">
                <Smartphone className="h-3 w-3" />
                <span>HWID</span>
              </span>
              <span className="font-mono tabular-nums text-foreground">
                {client._count?.devices ?? 0} / {client.deviceLimit || '∞'}
              </span>
            </div>
            <div className="flex items-center justify-between text-[11px] text-muted-foreground">
              <span>{isRu ? 'Активность' : 'Last active'}</span>
              <span className="font-mono tabular-nums">
                {client.lastActiveAt ? formatRelativeAgo(client.lastActiveAt, isRu) : '—'}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
