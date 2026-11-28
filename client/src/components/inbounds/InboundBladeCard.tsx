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

  const inlineSpecs = meta.specs
    .filter((s) => s.value && s.value !== '—' && s.value !== meta.endpoint.display)
    .slice(0, 3);

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
        'node-blade relative overflow-hidden rounded-xl border border-border/80 transition-colors',
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

      {/* Row 1: protocol icon, identity, subtitle & control dock */}
      <div
        className={cn(
          'flex items-center gap-2 bg-gradient-to-r p-2 pl-3.5',
          meta.theme.headerWashClass,
        )}
      >
        <div
          className={cn(
            'flex h-6 w-6 shrink-0 items-center justify-center rounded-md border',
            meta.theme.iconBoxClass,
          )}
          title={meta.protocolLabel}
        >
          <ProtocolIcon className="h-3.5 w-3.5" />
        </div>

        <Badge
          className={cn(
            'shrink-0 border px-1.5 py-0 text-[10px] font-semibold',
            PROTOCOL_COLORS[inbound.protocol],
          )}
        >
          {meta.protocolLabel}
        </Badge>

        <button
          type="button"
          onClick={() => onEdit(inbound)}
          className="min-w-0 truncate text-left text-sm font-bold tracking-tight transition-colors hover:text-primary"
          title={inbound.remark?.trim() ? `${inbound.tag} — ${inbound.remark.trim()}` : inbound.tag}
        >
          {inbound.tag}
        </button>

        <span
          className="hidden shrink-0 truncate font-mono text-[10px] text-muted-foreground sm:inline max-w-[11rem]"
          title={meta.subtitle}
        >
          {meta.subtitle}
        </span>

        <span className="flex-1" />

        <Badge variant={inbound.enable ? 'success' : 'secondary'} className="shrink-0 text-[10px]">
          {inbound.enable ? t('common.enabled') : t('common.disabled')}
        </Badge>

        <div className="flex shrink-0 items-center gap-1">
          <Badge
            variant="outline"
            className="hidden gap-1 font-mono text-[10px] sm:inline-flex"
            title={meta.coreLabel}
          >
            <Cpu className="h-3 w-3 text-primary" />
            {meta.coreLabel}
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

      {/* Row 2: compact protocol-aware endpoint, inline specs, flags, node */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/40 px-2 py-1.5 pl-3.5 text-[11px]">
        <button
          type="button"
          onClick={handleCopy}
          className="inline-flex min-w-0 max-w-[16rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={`${meta.endpoint.label}: ${meta.endpoint.copyValue}`}
        >
          <span className="truncate">{meta.endpoint.display}</span>
          {copiedEndpoint ? (
            <Check className="h-3 w-3 shrink-0 text-emerald-400" />
          ) : (
            <Copy className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
        </button>

        {meta.endpoint.secondary && (
          <Badge
            variant="outline"
            className="shrink-0 px-1.5 py-0 font-mono text-[10px] text-muted-foreground"
            title={`${meta.endpoint.secondary.label}: ${meta.endpoint.secondary.value}`}
          >
            {meta.endpoint.secondary.label}:{meta.endpoint.secondary.value}
          </Badge>
        )}

        {inlineSpecs.map((spec) => (
          <Badge
            key={spec.label}
            variant="secondary"
            className={cn(
              'shrink-0 max-w-[11rem] truncate px-1.5 py-0 text-[10px]',
              spec.mono && 'font-mono',
              specToneClass(spec.tone, meta.theme.accentTextClass),
            )}
            title={`${spec.label}: ${spec.title ?? spec.value}`}
          >
            {spec.value}
          </Badge>
        ))}

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

        <Link
          to={`/nodes/${inbound.nodeId}`}
          className="inline-flex min-w-0 max-w-[14rem] items-center gap-1 rounded border border-border/70 bg-background/60 px-1.5 py-0.5 font-medium text-foreground transition-colors hover:border-primary/45 hover:text-primary"
          title={inbound.node?.name ?? inbound.nodeId}
        >
          <Server className="h-3 w-3 shrink-0 text-primary" />
          <span className="truncate">{inbound.node?.name ?? inbound.nodeId}</span>
        </Link>
      </div>
    </div>
  );
}
