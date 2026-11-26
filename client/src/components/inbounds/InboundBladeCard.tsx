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
  xray: 'Xray',
  singbox: 'Sing-box',
  naive: 'Naive',
  mieru: 'Mieru',
  telemt: 'telemt',
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

      {/* Row 1: identity + control dock */}
      <div className="flex items-center gap-2 p-2 pl-3.5">
        <Badge className={cn('shrink-0 border font-semibold', PROTOCOL_COLORS[inbound.protocol])}>
          {inbound.protocol}
        </Badge>
        <button
          type="button"
          onClick={() => onEdit(inbound)}
          className="min-w-0 flex-1 break-words text-left text-sm font-bold tracking-tight transition-colors hover:text-primary"
        >
          {inbound.tag}
        </button>
        <Badge variant={inbound.enable ? 'success' : 'secondary'} className="shrink-0 text-[10px]">
          {inbound.enable ? t('common.enabled') : t('common.disabled')}
        </Badge>

        <div className="flex shrink-0 items-center gap-1">
          <Badge variant="outline" className="hidden gap-1 font-mono text-[10px] sm:inline-flex" title={CORE_LABELS[core] ?? core}>
            <Cpu className="h-3 w-3 text-primary" />
            {CORE_LABELS[core] ?? core}
          </Badge>
          <Button
            type="button"
            size="icon"
            variant="outline"
            className="h-7 w-7"
            onClick={() => onEdit(inbound)}
            aria-label={t('common.edit')}
            title={t('common.edit')}
          >
            <Edit className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            size="icon"
            variant="outline"
            className={cn('h-7 w-7', inbound.enable ? 'text-emerald-500' : 'text-muted-foreground')}
            onClick={() => onToggle(inbound.id)}
            aria-label={inbound.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}
            title={inbound.enable ? t('inbounds.turnOff') : t('inbounds.turnOn')}
          >
            <Power className="h-3.5 w-3.5" />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="ghost" size="icon" className="h-7 w-7" aria-label={t('common.actions')}>
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

      {/* Row 2: compact vitals — endpoint, transport, sni, node */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/40 px-2 py-1.5 pl-3.5 text-[11px]">
        <button
          type="button"
          onClick={handleCopy}
          className="inline-flex min-w-0 max-w-[15rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={isRu ? 'Скопировать адрес:порт' : 'Copy host:port'}
        >
          <span className="break-all">{endpointStr}</span>
          {copiedEndpoint ? (
            <Check className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
        </button>

        <Badge variant="secondary" className="shrink-0 font-mono text-[10px]">
          {network}
        </Badge>
        {security !== 'NONE' && (
          <Badge variant="outline" className="shrink-0 border-emerald-500/35 font-mono text-[10px] text-emerald-400">
            <Shield className="mr-0.5 h-2.5 w-2.5" />
            {security}
          </Badge>
        )}
        {sni && (
          <span
            className="hidden max-w-[12rem] truncate font-mono text-[11px] text-muted-foreground md:inline"
            title={sni}
          >
            SNI: {sni}
          </span>
        )}
        {flow && (
          <Badge variant="outline" className="shrink-0 font-mono text-[10px]">
            {flow}
          </Badge>
        )}
        {!inbound.sniffing && (
          <Badge variant="secondary" className="shrink-0 font-mono text-[10px]">
            sniff:OFF
          </Badge>
        )}
        {inbound.portShares?.length ? (
          <Badge variant="outline" className="shrink-0 font-mono text-[10px]">
            <Network className="mr-1 h-2.5 w-2.5" />
            {inbound.portShares.length}
          </Badge>
        ) : null}
        {inbound.routing?.blockTorrent && (
          <Badge variant="warning" className="shrink-0 text-[10px]">
            No-Torrent
          </Badge>
        )}

        <span className="flex-1" />

        <Link
          to={`/nodes/${inbound.nodeId}`}
          className="inline-flex min-w-0 max-w-[14rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-medium text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={inbound.node?.name ?? inbound.nodeId}
        >
          <Server className="h-3 w-3 shrink-0 text-primary" />
          <span className="break-words">{inbound.node?.name ?? inbound.nodeId}</span>
        </Link>
      </div>
    </div>
  );
}
