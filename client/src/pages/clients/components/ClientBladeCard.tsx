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
import { formatRelativeAgo } from '@/components/nodes/NodeTelemetryPrimitives';
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

  const statusBadge = client.banned ? (
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
  );

  const devices = client._count?.devices ?? 0;

  return (
    <div
      className={cn(
        'node-blade relative overflow-hidden rounded-xl border border-border/80 transition-colors hover:border-primary/45',
        selected && 'border-primary/60 ring-1 ring-primary/30',
      )}
    >
      <div className={cn('absolute inset-y-0 left-0 w-1', railClass)} aria-hidden="true" />

      {/* Row 1: identity + quick action dock */}
      <div className="flex items-center gap-2 p-2 pl-3.5">
        <Checkbox
          checked={selected}
          onCheckedChange={onToggleSelect}
          aria-label={`Select ${client.username}`}
        />
        <button
          type="button"
          onClick={onEdit}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-primary/25 bg-primary/12 font-mono text-[11px] font-bold uppercase text-primary transition-colors hover:border-primary/50"
          title={t('common.edit')}
        >
          {client.username.slice(0, 2)}
        </button>
        <button
          type="button"
          onClick={onEdit}
          className="min-w-0 flex-1 break-words text-left text-sm font-bold tracking-tight transition-colors hover:text-primary"
        >
          {client.username}
        </button>
        {statusBadge}
        {client.plan?.name && (
          <Badge variant="secondary" className="hidden shrink-0 text-[10px] sm:inline-flex">
            {client.plan.name}
          </Badge>
        )}

        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            size="icon"
            variant="outline"
            className="h-7 w-7 border-primary/25 bg-primary/10 text-primary hover:bg-primary/20"
            onClick={onOpenSubscription}
            aria-label={t('clients.keysQr')}
            title={t('clients.keysQr')}
          >
            <QrCode className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            size="icon"
            variant="outline"
            className="h-7 w-7"
            onClick={onEditNodes}
            aria-label={t('clients.bindNodesAction')}
            title={isRu ? 'Привязать ноды' : 'Bind nodes'}
          >
            <Server className="h-3.5 w-3.5 text-primary" />
          </Button>
          <Button size="icon" variant="outline" asChild className="h-7 w-7">
            <Link to={`/clients/${client.id}/traffic`} aria-label={t('clients.trafficChart')} title={isRu ? 'Трафик' : 'Traffic'}>
              <Activity className="h-3.5 w-3.5" />
            </Link>
          </Button>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="h-7 w-7"
            onClick={onEdit}
            aria-label={t('common.edit')}
            title={t('common.edit')}
          >
            <Edit className="h-3.5 w-3.5" />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="h-7 w-7"
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

      {/* Row 2: compact vitals — uuid, quota, expire, devices, activity */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/40 px-2 py-1.5 pl-3.5 text-[11px]">
        <button
          type="button"
          onClick={onCopyUuid}
          aria-label={`Copy UUID for ${client.username}`}
          className="inline-flex items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={isRu ? 'Скопировать UUID' : 'Copy UUID'}
        >
          <span>{client.uuid.slice(0, 8)}…</span>
          {copiedUuid ? (
            <Check className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
        </button>

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0.5 font-mono text-[11px] font-medium tabular-nums',
            limit > 0 && pct >= 90 ? 'text-red-500' : pct >= 75 ? 'text-amber-500' : 'text-foreground',
          )}
          title={t('clients.colTraffic')}
        >
          {formatBytes(used, { compact: true, locale: language })}
          {' / '}
          {limit > 0 ? formatBytes(limit, { compact: true, locale: language }) : '∞'}
          {limit > 0 && <span className="text-muted-foreground"> ({pct}%)</span>}
        </span>

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0.5 font-mono text-[11px] font-semibold tabular-nums',
            isExpired ? 'border-red-500/50 bg-red-500/12 text-red-600 dark:text-red-300' : isExpiringSoon ? 'border-amber-500/45 bg-amber-500/12 text-amber-600 dark:text-amber-300' : 'border-border/70 text-foreground',
          )}
          title={t('clients.colExpires')}
        >
          <Calendar className="h-3 w-3 shrink-0" />
          {client.expireAt ? formatDate(client.expireAt, undefined, timezone) : '∞'}
        </span>

        <span
          className="inline-flex items-center gap-1 rounded border border-border/70 bg-muted/40 px-1.5 py-0.5 font-mono text-[11px] tabular-nums text-foreground"
          title="HWID"
        >
          <Smartphone className="h-3 w-3 shrink-0" />
          {devices} / {client.deviceLimit || '∞'}
        </span>

        <div className="hidden min-w-0 flex-wrap items-center gap-1 lg:flex">
          {(client.protocols ?? []).slice(0, 4).map((proto) => (
            <Badge
              key={proto}
              className={cn('shrink-0 border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[proto])}
            >
              {proto}
            </Badge>
          ))}
          {(client.protocols?.length ?? 0) > 4 && (
            <Badge variant="outline" className="px-1 py-0 text-[10px]">
              +{(client.protocols?.length ?? 0) - 4}
            </Badge>
          )}
        </div>

        <span className="flex-1" />

        <span
          className={cn(
            'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[11px] tabular-nums',
            isExpired
              ? 'border-red-500/45 bg-red-500/12 text-red-600 dark:text-red-300'
              : 'border-border/70 bg-muted/40 text-muted-foreground',
          )}
          title={isRu ? 'Последняя активность' : 'Last active'}
        >
          {client.lastActiveAt ? formatRelativeAgo(client.lastActiveAt, isRu) : '—'}
        </span>
        <span
          className={cn('font-mono text-[11px] tabular-nums', isExpired ? 'text-red-500' : 'text-muted-foreground')}
          title={limit > 0 ? t('clients.colTraffic') : isRu ? 'Безлимит' : 'Unlimited'}
        >
          {limit > 0 ? `${pct}%` : '∞'}
        </span>
        <Button size="icon" variant="ghost" className="h-7 w-7" onClick={onEdit} aria-label={t('common.edit')} title={isRu ? 'Открыть клиента' : 'Open client'}>
          <Edit className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}
