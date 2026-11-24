import { Link } from 'react-router-dom';
import {
  Activity,
  Ban,
  Check,
  Copy,
  Edit,
  MoreHorizontal,
  QrCode,
  RotateCcw,
  Server,
  ShieldCheck,
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
import {
  PROTOCOL_COLORS,
  cn,
  formatBytes,
  formatDate,
} from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useUiStore } from '@/lib/store';
import type { Client } from '@/types';

interface ClientRowProps {
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

export function ClientRow({
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
}: ClientRowProps) {
  const { t } = useTranslation();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);

  const used = Number(client.usedTraffic || 0);
  const limit = Number(client.trafficLimit || 0);
  const pct = limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;

  return (
    <tr className="border-b border-border/60 last:border-b-0 hover:bg-accent/35 transition-colors">
      <td className="py-1.5 px-2">
        <Checkbox
          checked={selected}
          onCheckedChange={onToggleSelect}
          aria-label={`Select ${client.username}`}
        />
      </td>
      <td className="py-1.5 px-2">
        <button
          type="button"
          onClick={onEdit}
          className="font-semibold text-left hover:text-primary transition-colors"
        >
          {client.username}
        </button>
        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <button
            type="button"
            onClick={onCopyUuid}
            aria-label={`Copy UUID for ${client.username}`}
            className="inline-flex items-center gap-1 rounded px-1 py-0.5 font-mono hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <span>{client.uuid.slice(0, 8)}...</span>
            {copiedUuid ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
          </button>
          {client.email && <span className="break-words min-w-0">{client.email}</span>}
        </div>
      </td>
      <td className="py-1.5 px-2">
        <div className="flex flex-wrap gap-1">
          {(client.protocols ?? []).map((proto) => (
            <Badge key={proto} className={cn('border text-[10px]', PROTOCOL_COLORS[proto])}>
              {proto}
            </Badge>
          ))}
        </div>
      </td>
      <td className="py-1.5 px-2">
        <button
          type="button"
          onClick={onEditNodes}
          className="text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded"
          title={t('clients.bindNodesAction')}
        >
          {client.allNodes !== false ? (
            <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground hover:border-primary/50 hover:text-foreground">
              {t('clients.allNodesBadge')}
            </Badge>
          ) : (client.nodes?.length ?? 0) === 0 ? (
            <Badge variant="warning" className="text-[10px]">
              {t('clients.noNodesBadge')}
            </Badge>
          ) : (
            <div
              className="flex flex-wrap gap-1"
              title={(client.nodes ?? []).map((n) => n.name).join(', ')}
            >
              {(client.nodes ?? []).slice(0, 2).map((n) => (
                <Badge key={n.id} variant="secondary" className="text-[10px] break-words hover:border-primary/40 min-w-0">
                  {n.name}
                </Badge>
              ))}
              {(client.nodes?.length ?? 0) > 2 && (
                <Badge variant="outline" className="text-[10px]">
                  +{(client.nodes?.length ?? 0) - 2}
                </Badge>
              )}
            </div>
          )}
        </button>
      </td>
      <td className="py-1.5 px-2 min-w-[145px]">
        <div className="text-xs tabular-nums">
          {formatBytes(used, { locale: language })} /{' '}
          {limit > 0 ? formatBytes(limit, { locale: language }) : '∞'}
        </div>
        {limit > 0 && (
          <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={cn(
                'h-full rounded-full transition-all',
                pct >= 90 ? 'bg-red-500' : pct >= 70 ? 'bg-amber-500' : 'bg-primary',
              )}
              style={{ width: `${pct}%` }}
            />
          </div>
        )}
      </td>
      <td className="py-1.5 px-2 text-xs">
        <div>{client.expireAt ? formatDate(client.expireAt, undefined, timezone) : t('common.never')}</div>
        {client.resetStrategy && client.resetStrategy !== 'NO_RESET' && (
          <Badge variant="outline" className="mt-0.5 text-[10px]">
            {client.resetStrategy}
          </Badge>
        )}
      </td>
      <td className="py-1.5 px-2">
        {client.banned ? (
          <Badge variant="destructive">{t('common.banned')}</Badge>
        ) : client.onHold ? (
          <Badge variant="warning">{t('common.onHold')}</Badge>
        ) : (
          <Badge variant="success">{t('common.active')}</Badge>
        )}
      </td>
      <td className="py-1.5 px-2 text-right">
        <div className="inline-flex items-center gap-1.5">
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
            className="h-8 gap-1.5 px-2.5 text-xs"
            onClick={onEdit}
          >
            <Edit className="h-3.5 w-3.5" />
            <span>{t('common.edit')}</span>
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
      </td>
    </tr>
  );
}
