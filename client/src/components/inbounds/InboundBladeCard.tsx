import { useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Check,
  Copy,
  Cpu,
  Edit,
  MoreHorizontal,
  Network,
  Power,
  Server,
  Shield,
  Trash2,
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
import { PROTOCOL_COLORS, cn, getCoreForProtocol } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import type { Inbound } from '@/types';

interface InboundBladeCardProps {
  inbound: Inbound;
  isRu: boolean;
  onEdit: (ib: Inbound) => void;
  onToggle: (id: string) => void;
  onDelete: (id: string) => void;
}

const CORE_LABELS: Record<string, string> = {
  xray: 'Xray-core',
  singbox: 'Sing-box',
  naive: 'NaiveProxy',
  mieru: 'Mieru (mita)',
};

export function InboundBladeCard({
  inbound,
  isRu,
  onEdit,
  onToggle,
  onDelete,
}: InboundBladeCardProps) {
  const { t } = useTranslation();
  const [copiedEndpoint, setCopiedEndpoint] = useState(false);

  const core = getCoreForProtocol(inbound.protocol);
  const endpointHost = inbound.node?.host || inbound.listen || '0.0.0.0';
  const endpointStr = `${endpointHost}:${inbound.port}`;

  const stream = (inbound.stream ?? {}) as Record<string, unknown>;
  const settings = (inbound.settings ?? {}) as Record<string, unknown>;
  const network = typeof stream.network === 'string' ? stream.network.toUpperCase() : 'TCP';
  const security = typeof stream.security === 'string' ? stream.security.toUpperCase() : 'NONE';
  const flow = typeof settings.flow === 'string' && settings.flow ? settings.flow : null;

  const realitySettings = (stream.realitySettings ?? {}) as Record<string, unknown>;
  const tlsSettings = (stream.tlsSettings ?? {}) as Record<string, unknown>;
  const serverNames = Array.isArray(realitySettings.serverNames)
    ? (realitySettings.serverNames as string[])
    : [];
  const sni =
    serverNames[0] ||
    (typeof tlsSettings.serverName === 'string' ? tlsSettings.serverName : null) ||
    (typeof stream.sni === 'string' ? stream.sni : null);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(endpointStr);
      setCopiedEndpoint(true);
      window.setTimeout(() => setCopiedEndpoint(false), 1400);
    } catch {
      // clipboard optional
    }
  };

  return (
    <div
      className={cn(
        'node-blade relative overflow-hidden rounded-xl border border-border/80 transition-colors hover:border-primary/45',
        !inbound.enable && 'opacity-75',
      )}
    >
      <div
        className={cn(
          'absolute inset-y-0 left-0 w-1',
          inbound.enable
            ? 'bg-emerald-400 shadow-[0_0_12px_hsl(162_84%_43%/0.6)]'
            : 'bg-zinc-600',
        )}
        aria-hidden="true"
      />

      <div className="space-y-3.5 p-4 pl-5">
        {/* Header Row */}
        <div className="flex flex-wrap items-start justify-between gap-2.5">
          <div className="min-w-0 space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <Badge className={cn('border font-semibold', PROTOCOL_COLORS[inbound.protocol])}>
                {inbound.protocol}
              </Badge>
              <button
                type="button"
                onClick={() => onEdit(inbound)}
                className="text-base font-bold tracking-tight hover:text-primary transition-colors text-left break-words min-w-0"
              >
                {inbound.tag}
              </button>
              <Badge variant={inbound.enable ? 'success' : 'secondary'} className="text-[10px]">
                {inbound.enable ? t('common.enabled') : t('common.disabled')}
              </Badge>
            </div>

            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <button
                type="button"
                onClick={handleCopy}
                className="inline-flex items-center gap-1.5 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 font-mono text-xs text-foreground transition-colors hover:border-primary/45 hover:text-primary"
                title={isRu ? 'Скопировать адрес:порт' : 'Copy host:port'}
              >
                <span>{endpointStr}</span>
                {copiedEndpoint ? (
                  <Check className="h-3 w-3 text-emerald-400" />
                ) : (
                  <Copy className="h-3 w-3 text-muted-foreground" />
                )}
              </button>
              <span className="font-mono text-[11px]">
                listen {inbound.listen}:{inbound.port}
              </span>
            </div>
          </div>

          {/* Engine Badge & Controls */}
          <div className="flex items-center gap-1.5">
            <Badge variant="outline" className="gap-1 font-mono text-[10px]">
              <Cpu className="h-3 w-3 text-primary" />
              {CORE_LABELS[core] ?? core}
            </Badge>

            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 gap-1 px-2.5 text-xs"
              onClick={() => onEdit(inbound)}
            >
              <Edit className="h-3.5 w-3.5" />
              <span>{t('common.edit')}</span>
            </Button>

            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="h-8 gap-1 px-2.5 text-xs"
              onClick={() => onToggle(inbound.id)}
            >
              <Power className="h-3.5 w-3.5" />
              <span>{inbound.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}</span>
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={t('common.actions')}>
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-44">
                <DropdownMenuItem onClick={() => onEdit(inbound)}>
                  <Edit className="h-3.5 w-3.5" />
                  <span>{t('common.edit')}</span>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onToggle(inbound.id)}>
                  <Power className="h-3.5 w-3.5" />
                  <span>{inbound.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}</span>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => onDelete(inbound.id)}
                  className="text-red-500 dark:text-red-400 focus:text-red-500 dark:focus:text-red-400"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  <span>{t('common.delete')}</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        {/* 3-Cell Technical Spec Grid */}
        <div className="grid gap-2.5 sm:grid-cols-3 text-xs">
          {/* Target Node */}
          <div className="rack-inset rounded-lg p-2.5 space-y-1">
            <div className="text-[11px] text-muted-foreground">{isRu ? 'Целевая нода' : 'Target Node'}</div>
            <Link
              to={`/nodes/${inbound.nodeId}`}
              className="inline-flex items-center gap-1.5 font-semibold text-foreground hover:text-primary transition-colors"
            >
              <Server className="h-3.5 w-3.5 text-primary" />
              <span className="break-words min-w-0">{inbound.node?.name ?? inbound.nodeId}</span>
            </Link>
          </div>

          {/* Transport & TLS */}
          <div className="rack-inset rounded-lg p-2.5 space-y-1">
            <div className="text-[11px] text-muted-foreground">
              {isRu ? 'Транспорт и защита' : 'Transport & Security'}
            </div>
            <div className="flex flex-wrap items-center gap-1">
              <Badge variant="secondary" className="font-mono text-[10px]">
                {network}
              </Badge>
              {security !== 'NONE' && (
                <Badge variant="outline" className="font-mono text-[10px] border-emerald-500/35 text-emerald-400">
                  <Shield className="mr-0.5 h-2.5 w-2.5" />
                  {security}
                </Badge>
              )}
              {flow && (
                <Badge variant="outline" className="font-mono text-[10px]">
                  {flow}
                </Badge>
              )}
              {sni && (
                <span className="break-words font-mono text-[11px] text-muted-foreground min-w-0" title={sni}>
                  SNI: {sni}
                </span>
              )}
            </div>
          </div>

          {/* Port-Shares & Sniffing */}
          <div className="rack-inset rounded-lg p-2.5 space-y-1">
            <div className="text-[11px] text-muted-foreground">
              {isRu ? 'Опции и Port-Share' : 'Options & Port-Share'}
            </div>
            <div className="flex flex-wrap items-center gap-1">
              <Badge variant="outline" className="font-mono text-[10px]">
                <Network className="mr-1 h-2.5 w-2.5" />
                {inbound.portShares?.length ?? 0} {isRu ? 'доп.' : 'shares'}
              </Badge>
              <Badge
                variant={inbound.sniffing ? 'success' : 'secondary'}
                className="font-mono text-[10px]"
              >
                Sniffing: {inbound.sniffing ? 'ON' : 'OFF'}
              </Badge>
              {inbound.routing?.blockTorrent && (
                <Badge variant="warning" className="text-[10px]">
                  No-Torrent
                </Badge>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
