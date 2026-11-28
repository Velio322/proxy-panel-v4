import { Link } from 'react-router-dom';
import {
  Check,
  Copy,
  Cpu,
  Edit,
  EyeOff,
  Fingerprint,
  Gauge,
  Globe,
  KeyRound,
  Layers,
  Lock,
  MoreHorizontal,
  Power,
  Send,
  Server,
  Shield,
  ShieldCheck,
  Shuffle,
  Trash2,
  Users,
  Zap,
  type LucideIcon,
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
  extractProtocolCardMeta,
  type ProtocolFlag,
  type ProtocolIconKey,
  type SpecTone,
} from '@/components/inbounds/protocolCardMeta';
import { PROTOCOL_COLORS, cn } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import type { Inbound } from '@/types';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

interface InboundBladeCardProps {
  inbound: Inbound;
  isRu: boolean;
  onEdit: (ib: Inbound) => void;
  onToggle: (id: string) => void;
  onDelete: (id: string) => void;
}

const PROTOCOL_ICONS: Record<ProtocolIconKey, LucideIcon> = {
  vless: Shield,
  reality: Fingerprint,
  vmess: Layers,
  trojan: KeyRound,
  shadowsocks: Lock,
  hysteria2: Zap,
  naiveproxy: Globe,
  mieru: Shuffle,
  tuic: Gauge,
  anytls: ShieldCheck,
  shadowtls: EyeOff,
  tgweb: Send,
};

function specToneClass(tone: SpecTone | undefined, accentTextClass: string): string {
  switch (tone) {
    case 'emerald':
      return 'text-emerald-600 dark:text-emerald-400';
    case 'amber':
      return 'text-amber-600 dark:text-amber-400';
    case 'accent':
      return accentTextClass;
    case 'muted':
      return 'text-muted-foreground';
    default:
      return 'text-foreground';
  }
}

function flagVariant(tone: ProtocolFlag['tone']): 'outline' | 'success' | 'warning' | 'default' {
  switch (tone) {
    case 'emerald':
      return 'success';
    case 'amber':
      return 'warning';
    case 'accent':
      return 'default';
    default:
      return 'outline';
  }
}

export function InboundBladeCard({
  inbound,
  isRu,
  onEdit,
  onToggle,
  onDelete,
}: InboundBladeCardProps) {
  const { t } = useTranslation();
  const [copiedEndpoint, copyEndpoint] = useCopyFeedback(1400);
  const meta = extractProtocolCardMeta(inbound, isRu);
  const ProtocolIcon = PROTOCOL_ICONS[meta.icon] ?? Shield;

  const handleCopy = async () => {
    try {
      await copyEndpoint(meta.endpoint.copyValue);
    } catch {
      // clipboard optional
    }
  };

  return (
    <div
      data-protocol={inbound.protocol}
      className={cn(
        'node-blade relative flex flex-col overflow-hidden rounded-xl border border-border/80 transition-colors',
        meta.theme.cardHoverClass,
        !inbound.enable && 'opacity-75',
      )}
    >
      <div
        className={cn(
          'absolute inset-y-0 left-0 w-1',
          inbound.enable ? meta.theme.railClass : 'bg-zinc-600/80',
        )}
        aria-hidden="true"
      />

      {/* Row 1: protocol identity + control dock */}
      <div
        className={cn(
          'flex items-start gap-2.5 bg-gradient-to-r p-2.5 pl-4',
          meta.theme.headerWashClass,
        )}
      >
        <div
          className={cn(
            'mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border',
            meta.theme.iconBoxClass,
          )}
          title={meta.protocolLabel}
        >
          <ProtocolIcon className="h-4 w-4" />
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={() => onEdit(inbound)}
              className="min-w-0 break-words text-left text-sm font-bold tracking-tight transition-colors hover:text-primary"
            >
              {inbound.tag}
            </button>
            <Badge variant={inbound.enable ? 'success' : 'secondary'} className="shrink-0 text-[10px]">
              {inbound.enable ? t('common.enabled') : t('common.disabled')}
            </Badge>
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            <Badge
              className={cn(
                'shrink-0 border px-1.5 py-0 text-[10px] font-semibold',
                PROTOCOL_COLORS[inbound.protocol],
              )}
            >
              {meta.protocolLabel}
            </Badge>
            <span className="truncate font-mono text-[11px] text-muted-foreground" title={meta.subtitle}>
              {meta.subtitle}
            </span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1">
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
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                aria-label={t('common.actions')}
              >
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

      {/* Row 2: primary protocol-aware endpoint strip */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/45 px-3 py-1.5 pl-4 text-[11px]">
        <button
          type="button"
          onClick={handleCopy}
          className="inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-md border border-border/70 bg-background/70 px-2 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={meta.endpoint.copyValue}
        >
          <span className="shrink-0 font-sans text-[10px] uppercase tracking-wider text-muted-foreground">
            {meta.endpoint.label}
          </span>
          <span className="truncate font-semibold">{meta.endpoint.display}</span>
          {copiedEndpoint ? (
            <Check className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
        </button>

        {meta.endpoint.secondary && (
          <Badge
            variant="outline"
            className="shrink-0 font-mono text-[10px] text-muted-foreground"
            title={`${meta.endpoint.secondary.label}: ${meta.endpoint.secondary.value}`}
          >
            <span className="font-sans text-muted-foreground/80">{meta.endpoint.secondary.label}:</span>
            <span>{meta.endpoint.secondary.value}</span>
          </Badge>
        )}
      </div>

      {/* Row 3: tailored protocol-specific settings grid */}
      <div className="grid grid-cols-2 gap-1.5 px-3 py-2 pl-4">
        {meta.specs.map((spec) => (
          <div
            key={spec.label}
            className="min-w-0 rounded-lg border border-border/55 bg-background/50 px-2.5 py-1.5"
          >
            <div className="truncate text-[10px] uppercase tracking-wider text-muted-foreground leading-none">
              {spec.label}
            </div>
            <div
              className={cn(
                'mt-1 truncate text-xs font-medium',
                spec.mono && 'font-mono',
                specToneClass(spec.tone, meta.theme.accentTextClass),
              )}
              title={spec.title ?? spec.value}
            >
              {spec.value}
            </div>
          </div>
        ))}
      </div>

      {inbound.remark?.trim() && (
        <div
          className="truncate px-3 pb-1.5 pl-4 text-[11px] text-muted-foreground"
          title={inbound.remark.trim()}
        >
          {inbound.remark.trim()}
        </div>
      )}

      {/* Row 4: protocol-specific flags + clients + engine + node */}
      <div className="mt-auto flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/35 px-3 py-1.5 pl-4 text-[11px]">
        {meta.flags.map((flag) => (
          <Badge
            key={flag.label}
            variant={flagVariant(flag.tone)}
            className="shrink-0 px-1.5 py-0 font-mono text-[10px]"
            title={flag.title ?? flag.label}
          >
            {flag.label}
          </Badge>
        ))}

        <span className="flex-1" />

        {typeof inbound._count?.clients === 'number' && (
          <Badge
            variant="outline"
            className="shrink-0 gap-1 px-1.5 py-0 font-mono text-[10px] text-muted-foreground"
            title={isRu ? 'Подключённые клиенты' : 'Assigned clients'}
          >
            <Users className="h-2.5 w-2.5" />
            {inbound._count.clients}
          </Badge>
        )}

        <Badge
          variant="outline"
          className="hidden shrink-0 gap-1 px-1.5 py-0 font-mono text-[10px] sm:inline-flex"
          title={meta.coreLabel}
        >
          <Cpu className="h-3 w-3 text-primary" />
          {meta.coreLabel}
        </Badge>

        <Link
          to={`/nodes/${inbound.nodeId}`}
          className="inline-flex min-w-0 max-w-[12rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-medium text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={inbound.node?.name ?? inbound.nodeId}
        >
          <Server className="h-3 w-3 shrink-0 text-primary" />
          <span className="truncate">{inbound.node?.name ?? inbound.nodeId}</span>
        </Link>
      </div>
    </div>
  );
}
