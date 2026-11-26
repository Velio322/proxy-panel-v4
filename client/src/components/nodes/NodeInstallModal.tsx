import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Activity,
  Check,
  Copy,
  Globe,
  Layers,
  Loader2,
  Network,
  Radio,
  Server,
  Shield,
  Terminal,
  Wifi,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { getApiErrorMessage, nodesApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

export interface NodeInstallInitialData {
  nodeId?: string;
  name?: string;
  host?: string;
  port?: number;
  apiPort?: number;
  secret?: string;
  isDomain?: boolean;
  panelUrl?: string;
  wsUrl?: string;
  scriptUrl?: string;
  binaryUrl?: string;
  command?: string;
  installCommand?: string;
  setupInstructions?: string;
}

interface NodeInstallModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  nodeId?: string | null;
  initialData?: NodeInstallInitialData | null;
  defaultTab?: 'connect' | 'scenarios';
}

type TlsModeOption = 'none' | 'port2222' | 'auto';
type CoresOption = 'all' | 'xray,singbox' | 'xray' | 'none';

function shellSingleQuote(value: string): string {
  return value.replace(/'/g, "'\\''");
}

function CopySnippetBox({
  label,
  value,
  multiline = false,
  isRu,
}: {
  label?: string;
  value: string;
  multiline?: boolean;
  isRu: boolean;
}) {
  const [copied, copyValue] = useCopyFeedback(1600);

  const handleCopy = async () => {
    if (!value) return;
    try {
      await copyValue(value);
    } catch {
      toast({
        title: isRu ? 'Не удалось скопировать' : 'Copy failed',
        variant: 'destructive',
      });
    }
  };

  return (
    <div className="space-y-1.5">
      {label && <Label className="text-xs text-muted-foreground">{label}</Label>}
      <div className="relative rounded-xl border border-border/80 bg-background/90 p-2.5 pr-11">
        {multiline ? (
          <pre className="max-h-40 overflow-auto font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all text-foreground">
            {value}
          </pre>
        ) : (
          <code className="block overflow-x-auto font-mono text-[11px] leading-relaxed whitespace-nowrap text-foreground no-scrollbar">
            {value}
          </code>
        )}
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="absolute right-1.5 top-1.5 h-8 w-8 text-muted-foreground hover:text-foreground"
          onClick={handleCopy}
          title={isRu ? 'Копировать' : 'Copy'}
        >
          {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
        </Button>
      </div>
    </div>
  );
}

export function NodeInstallModal({
  open,
  onOpenChange,
  nodeId,
  initialData,
  defaultTab,
}: NodeInstallModalProps) {
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const qc = useQueryClient();

  const resolvedNodeId = nodeId ?? initialData?.nodeId ?? null;
  const [tab, setTab] = useState<'connect' | 'scenarios'>(
    defaultTab ?? (resolvedNodeId ? 'connect' : 'scenarios'),
  );

  const [tlsMode, setTlsMode] = useState<TlsModeOption>('none');
  const [tlsEmail, setTlsEmail] = useState('');
  const [coresOption, setCoresOption] = useState<CoresOption>('all');
  const [enableBbr, setEnableBbr] = useState(true);
  const [checkingNow, setCheckingNow] = useState(false);

  const notifiedOnlineRef = useRef(false);

  useEffect(() => {
    if (open) {
      setTab(defaultTab ?? (resolvedNodeId ? 'connect' : 'scenarios'));
      notifiedOnlineRef.current = false;
    }
  }, [open, defaultTab, resolvedNodeId]);

  const { data: cmdInfo } = useQuery({
    queryKey: ['node-install-cmd', resolvedNodeId],
    queryFn: () => nodesApi.getInstallCommand(resolvedNodeId!),
    enabled: open && Boolean(resolvedNodeId),
    initialData: initialData ?? undefined,
  });

  const { data: liveNode, refetch: refetchLiveNode } = useQuery({
    queryKey: ['node', resolvedNodeId],
    queryFn: () => nodesApi.get(resolvedNodeId!),
    enabled: open && Boolean(resolvedNodeId),
    refetchInterval: (query) => {
      if (!open || !resolvedNodeId) return false;
      const status = query.state.data?.status;
      return status === 'ONLINE' ? 10_000 : 3_000;
    },
  });

  useEffect(() => {
    if (!open || !liveNode) return;
    if (liveNode.status === 'ONLINE' && !notifiedOnlineRef.current) {
      notifiedOnlineRef.current = true;
      qc.invalidateQueries({ queryKey: ['nodes'] });
      toast({
        title: isRu
          ? `Нода "${liveNode.name}" подключена`
          : `Node "${liveNode.name}" is ONLINE`,
        description: isRu
          ? 'Агент установил WSS-туннель и синхронизировал конфигурацию.'
          : 'Worker agent connected via WSS and synced configuration.',
        variant: 'success',
      });
    }
  }, [open, liveNode, qc, isRu]);

  const panelUrl = useMemo(() => {
    const raw =
      cmdInfo?.panelUrl ??
      initialData?.panelUrl ??
      (typeof window !== 'undefined' ? window.location.origin : 'https://panel.example.com');
    return raw.replace(/\/+$/, '');
  }, [cmdInfo?.panelUrl, initialData?.panelUrl]);

  const nodeHost = cmdInfo?.host ?? initialData?.host ?? liveNode?.host ?? '';
  const nodeName = cmdInfo?.name ?? initialData?.name ?? liveNode?.name ?? '';
  const nodeApiPort = cmdInfo?.apiPort ?? initialData?.apiPort ?? liveNode?.apiPort ?? 2222;
  const nodeSecret = cmdInfo?.secret ?? initialData?.secret ?? '';
  const isDomain =
    cmdInfo?.isDomain ??
    initialData?.isDomain ??
    (Boolean(nodeHost) && !/^\d+\.\d+\.\d+\.\d+$/.test(nodeHost.trim()));

  const customCommand = useMemo(() => {
    if (!resolvedNodeId || !nodeHost || !nodeName || !nodeSecret) {
      return cmdInfo?.installCommand ?? cmdInfo?.command ?? initialData?.installCommand ?? '';
    }
    const parts = [
      `--mode node`,
      `--master-url '${shellSingleQuote(panelUrl)}'`,
      `--node-domain '${shellSingleQuote(nodeHost)}'`,
      `--node-name '${shellSingleQuote(nodeName)}'`,
      `--node-secret '${shellSingleQuote(nodeSecret)}'`,
    ];
    if (nodeApiPort !== 2222) {
      parts.push(`--api-port '${nodeApiPort}'`);
    }
    if (tlsMode !== 'none') {
      parts.push(`--tls-mode '${tlsMode}'`);
      if (tlsEmail.trim()) {
        parts.push(`--email '${shellSingleQuote(tlsEmail.trim())}'`);
      }
    }
    if (coresOption !== 'all') {
      parts.push(`--cores '${coresOption}'`);
    }
    if (!enableBbr) {
      parts.push(`--no-bbr`);
    }
    return `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' | sudo bash -s -- ${parts.join(' ')}`;
  }, [
    resolvedNodeId,
    nodeHost,
    nodeName,
    nodeSecret,
    nodeApiPort,
    panelUrl,
    tlsMode,
    tlsEmail,
    coresOption,
    enableBbr,
    cmdInfo?.installCommand,
    cmdInfo?.command,
    initialData?.installCommand,
  ]);

  const twoStepCommand = useMemo(() => {
    const flags = customCommand.split(' | sudo bash -s -- ')[1] || '';
    if (!flags) return cmdInfo?.setupInstructions ?? '';
    return [
      `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' -o /tmp/mypanel3-install.sh`,
      `sudo bash /tmp/mypanel3-install.sh ${flags}`,
      `rm -f /tmp/mypanel3-install.sh`,
    ].join('\n');
  }, [customCommand, panelUrl, cmdInfo?.setupInstructions]);

  const scenarioTemplates = useMemo(
    () => [
      {
        id: 'both',
        badge: isRu ? 'Сценарий 1' : 'Scenario 1',
        title: isRu ? 'Панель + Нода на одном сервере' : 'Panel + Node on one VPS',
        subtitle: isRu
          ? 'HAProxy SNI-mux делит :443 между HTTPS панели и Xray Reality без конфликтов. Автоматически регистрирует локальную ноду.'
          : 'HAProxy SNI-mux shares :443 cleanly between Panel HTTPS and Xray Reality, and auto-registers the local node.',
        ports: '22, 80, 443',
        command: `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' | sudo bash -s -- --mode both --domain panel.example.com --email admin@example.com --cores all`,
      },
      {
        id: 'panel',
        badge: isRu ? 'Сценарий 2' : 'Scenario 2',
        title: isRu ? 'Только Панель (Control Plane)' : 'Panel Only (Control Plane)',
        subtitle: isRu
          ? 'Минимальный сервер: PostgreSQL + mypanel3 + Caddy HTTPS. Прокси-ядра и агент не устанавливаются.'
          : 'Lean management plane: PostgreSQL + mypanel3 + Caddy HTTPS. No proxy cores or worker agent.',
        ports: '22, 80, 443',
        command: `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' | sudo bash -s -- --mode panel --domain panel.example.com --sub-domain sub.example.com --email admin@example.com`,
      },
      {
        id: 'node',
        badge: isRu ? 'Сценарий 3' : 'Scenario 3',
        title: isRu ? 'Только Нода (отдельный VPS)' : 'Node Only (Dedicated Proxy VPS)',
        subtitle: isRu
          ? 'По умолчанию --tls-mode none: порт 443 полностью свободен под VLESS Reality! Агент слушает порт управления 2222 и держит WSS к панели.'
          : 'Default --tls-mode none keeps :443 100% free for VLESS Reality. Worker agent listens on port 2222 and pairs over outbound WSS.',
        ports: isRu ? '2222 (API), 443 (VLESS), 8443 (Naive), 2053 (Mieru)' : '2222 (API), 443 (VLESS), 8443 (Naive), 2053 (Mieru)',
        command: `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' | sudo bash -s -- --mode node --master-url '${shellSingleQuote(panelUrl)}' --node-domain 203.0.113.10 --node-name EU-1 --node-secret '<SECRET_FROM_PANEL>'`,
      },
      {
        id: 'pair',
        badge: isRu ? 'Сценарий 4' : 'Scenario 4',
        title: isRu ? 'Подключение новой ноды к работающей панели' : 'Attach New Node to Running Panel',
        subtitle: isRu
          ? '1) Создайте ноду в UI панели (порт 2222). 2) Скопируйте команду или NODE_SECRET. 3) Выполните на чистом VPS — бинарник, ядра и конфиги встанут автоматически.'
          : '1) Add the node in Panel UI (port 2222). 2) Copy the generated 1-liner or NODE_SECRET. 3) Run on a fresh VPS — binary, cores and inbounds auto-sync.',
        ports: '2222 (API) + Outbound WSS -> Panel :443',
        command: customCommand || `curl -fsSL '${shellSingleQuote(panelUrl)}/install.sh' | sudo bash -s -- --mode node --master-url '${shellSingleQuote(panelUrl)}' --node-domain <NODE_IP_OR_DOMAIN> --node-name <NAME> --node-secret '<SECRET>'`,
      },
    ],
    [panelUrl, customCommand, isRu],
  );

  const handleManualCheck = async () => {
    if (!resolvedNodeId) return;
    setCheckingNow(true);
    try {
      await nodesApi.check(resolvedNodeId);
      await refetchLiveNode();
      qc.invalidateQueries({ queryKey: ['nodes'] });
    } catch (err) {
      toast({
        title: isRu ? 'Проверка не удалась' : 'Check failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setCheckingNow(false);
    }
  };

  const isOnline = liveNode?.status === 'ONLINE';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Terminal className="h-5 w-5 text-primary" />
            <span>
              {resolvedNodeId
                ? isRu
                  ? `Подключение ноды: ${nodeName || resolvedNodeId}`
                  : `Node Deployment & Pairing: ${nodeName || resolvedNodeId}`
                : isRu
                  ? 'Мастер установки: 4 сценария развёртывания'
                  : 'Installation Wizard: 4 Deployment Scenarios'}
            </span>
          </DialogTitle>
          <DialogDescription>
            {isRu
              ? 'Самообновляемый установщик с панели: бинарник mypanel3 загружается с этой панели и проверяется по SHA-256.'
              : 'Self-hosted installer served directly from this panel with SHA-256 binary verification.'}
          </DialogDescription>
        </DialogHeader>

        <Tabs value={tab} onValueChange={(v) => setTab(v as 'connect' | 'scenarios')}>
          <TabsList className="w-full justify-start">
            {resolvedNodeId && (
              <TabsTrigger value="connect">
                <Wifi className="h-3.5 w-3.5" />
                <span>{isRu ? 'Подключить эту ноду' : 'Pair This Node'}</span>
              </TabsTrigger>
            )}
            <TabsTrigger value="scenarios">
              <Layers className="h-3.5 w-3.5" />
              <span>{isRu ? '4 сценария установки' : '4 Setup Scenarios'}</span>
            </TabsTrigger>
          </TabsList>

          {resolvedNodeId && (
            <TabsContent value="connect" className="space-y-4">
              {/* Live Pairing Status */}
              <div
                className={cn(
                  'flex flex-wrap items-center justify-between gap-3 rounded-xl border px-3.5 py-2.5',
                  isOnline
                    ? 'border-emerald-500/40 bg-emerald-500/10'
                    : 'border-amber-500/35 bg-amber-500/10',
                )}
              >
                <div className="flex items-center gap-2.5">
                  {isOnline ? (
                    <span className="inline-flex h-2.5 w-2.5 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.9)]" />
                  ) : (
                    <Loader2 className="h-4 w-4 animate-spin text-amber-400" />
                  )}
                  <div className="space-y-0.5">
                    <div className="flex items-center gap-2 text-xs font-semibold">
                      <span>
                        {isOnline
                          ? isRu
                            ? 'Нода подключена (ONLINE)'
                            : 'Node Connected (ONLINE)'
                          : isRu
                            ? 'Ожидание подключения агента по WSS…'
                            : 'Waiting for agent WSS connection…'}
                      </span>
                      {liveNode?.arch && (
                        <Badge variant="outline" className="h-4 px-1.5 font-mono text-[10px]">
                          {liveNode.arch}
                        </Badge>
                      )}
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {isOnline
                        ? [
                            liveNode?.version && `v${liveNode.version}`,
                            liveNode?.xrayVersion && `xray ${liveNode.xrayVersion}`,
                            liveNode?.singboxVersion && `sing-box ${liveNode.singboxVersion}`,
                            liveNode?.naiveVersion && `naive ${liveNode.naiveVersion}`,
                            liveNode?.mieruVersion && `mieru ${liveNode.mieruVersion}`,
                            liveNode?.telemtVersion && `telemt ${liveNode.telemtVersion}`,
                          ]
                            .filter(Boolean)
                            .join(' • ') || (isRu ? 'Телеметрия активна' : 'Telemetry active')
                        : isRu
                          ? `Хост ${nodeHost || '—'} • выполните команду ниже на сервере ноды`
                          : `Host ${nodeHost || '—'} • run the command below on the target node VPS`}
                    </p>
                  </div>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handleManualCheck}
                  loading={checkingNow}
                >
                  <Activity className="h-3.5 w-3.5" />
                  <span>{isRu ? 'Проверить' : 'Check'}</span>
                </Button>
              </div>

              {/* Options Grid */}
              <div className="rack-inset space-y-3 rounded-xl p-2">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-xs font-medium text-foreground">
                    {isRu ? 'Параметры команды установки' : 'Installer Options'}
                  </span>
                  <span className="text-[11px] text-muted-foreground">
                    {isDomain
                      ? isRu
                        ? 'Домен: доступен порт 2222 TLS при необходимости'
                        : 'Domain host: port 2222 TLS optional'
                      : isRu
                        ? 'IP-хост: используется WSS и HTTP API :' + nodeApiPort
                        : 'IP host: uses outbound WSS & HTTP API :' + nodeApiPort}
                  </span>
                </div>

                <div className="grid gap-2 sm:grid-cols-2">
                  <div className="space-y-1.5">
                    <Label className="text-xs">{isRu ? 'Режим TLS агента (--tls-mode)' : 'Agent TLS Mode (--tls-mode)'}</Label>
                    <select
                      value={tlsMode}
                      onChange={(e) => setTlsMode(e.target.value as TlsModeOption)}
                      className="h-9 w-full rounded-lg border border-input bg-background px-2.5 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <option value="none">
                        {isRu
                          ? `none — WSS + API :${nodeApiPort} (:443 свободен)`
                          : `none — WSS + API :${nodeApiPort} (:443 free)`}
                      </option>
                      <option value="port2222" disabled={!isDomain}>
                        {isRu
                          ? `port2222 — Caddy TLS :${nodeApiPort} (нужен домен + :80)`
                          : `port2222 — Caddy TLS :${nodeApiPort} (domain + :80)`}
                      </option>
                      <option value="auto" disabled={!isDomain}>
                        {isRu
                          ? 'auto — Caddy TLS на :443 (занимает 443)'
                          : 'auto — Caddy TLS on :443 (occupies 443)'}
                      </option>
                    </select>
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs">{isRu ? 'Прокси-ядра (--cores)' : 'Proxy Cores (--cores)'}</Label>
                    <select
                      value={coresOption}
                      onChange={(e) => setCoresOption(e.target.value as CoresOption)}
                      className="h-9 w-full rounded-lg border border-input bg-background px-2.5 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <option value="all">
                        {isRu ? 'all — Xray, Sing-box, Naive, Mieru' : 'all — Xray, Sing-box, Naive, Mieru'}
                      </option>
                      <option value="xray,singbox">xray,singbox</option>
                      <option value="xray">xray</option>
                      <option value="none">{isRu ? 'none — только агент' : 'none — agent only'}</option>
                    </select>
                  </div>
                </div>

                {tlsMode !== 'none' && (
                  <div className="space-y-1.5">
                    <Label className="text-xs">
                      {isRu ? 'Email для Let’s Encrypt (--email)' : 'Let’s Encrypt Email (--email)'}
                    </Label>
                    <Input
                      value={tlsEmail}
                      onChange={(e) => setTlsEmail(e.target.value)}
                      placeholder="admin@example.com"
                      className="h-8 text-xs"
                    />
                  </div>
                )}

                <div className="flex items-center justify-between pt-0.5">
                  <div className="space-y-0.5">
                    <Label className="text-xs">{isRu ? 'Включить BBR и sysctl тюнинг' : 'Enable BBR & TCP sysctl tuning'}</Label>
                    <p className="text-[11px] text-muted-foreground">
                      {isRu
                        ? 'Оптимизирует TCP/UDP очереди для VLESS / Hysteria2 / Mieru'
                        : 'Tunes TCP/UDP buffers for VLESS / Hysteria2 / Mieru'}
                    </p>
                  </div>
                  <Switch checked={enableBbr} onCheckedChange={setEnableBbr} />
                </div>
              </div>

              <CopySnippetBox
                label={
                  isRu
                    ? 'Команда в одну строку (выполните под root или sudo на новом VPS)'
                    : 'One-Line Install & Pair Command (run on target VPS)'
                }
                value={customCommand}
                isRu={isRu}
              />

              {twoStepCommand && (
                <CopySnippetBox
                  label={
                    isRu
                      ? 'Безопасная установка в 2 шага (если нужно предварительно просмотреть скрипт)'
                      : 'Two-Step Auditable Install'
                  }
                  value={twoStepCommand}
                  multiline
                  isRu={isRu}
                />
              )}

              <div className="grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-3">
                <div className="rounded-lg border border-border/60 bg-muted/20 px-2.5 py-2">
                  <div className="flex items-center gap-1 font-medium text-foreground">
                    <Globe className="h-3 w-3 text-primary" />
                    <span>{isRu ? 'Панель' : 'Panel URL'}</span>
                  </div>
                  <div className="mt-0.5 break-words font-mono min-w-0">{panelUrl}</div>
                </div>
                <div className="rounded-lg border border-border/60 bg-muted/20 px-2.5 py-2">
                  <div className="flex items-center gap-1 font-medium text-foreground">
                    <Radio className="h-3 w-3 text-primary" />
                    <span>{isRu ? 'WSS транспорт' : 'WSS Transport'}</span>
                  </div>
                  <div className="mt-0.5 break-words font-mono min-w-0">{cmdInfo?.wsUrl || `${panelUrl.replace(/^http/, 'ws')}/ws/worker`}</div>
                </div>
                <div className="rounded-lg border border-border/60 bg-muted/20 px-2.5 py-2">
                  <div className="flex items-center gap-1 font-medium text-foreground">
                    <Shield className="h-3 w-3 text-primary" />
                    <span>{isRu ? `Порт API / Секрет` : `API Port / Secret`}</span>
                  </div>
                  <div className="mt-0.5 break-words font-mono min-w-0">
                    :{nodeApiPort} • {nodeSecret ? `${nodeSecret.slice(0, 8)}…` : '—'}
                  </div>
                </div>
              </div>

              {nodeSecret && (
                <CopySnippetBox
                  label={
                    isRu
                      ? `Секрет ноды (NODE_SECRET) для интерактивного мастера (порт управления: ${nodeApiPort})`
                      : `Node Secret (NODE_SECRET) for interactive installer (management port: ${nodeApiPort})`
                  }
                  value={nodeSecret}
                  isRu={isRu}
                />
              )}
            </TabsContent>
          )}

          <TabsContent value="scenarios" className="space-y-3">
            <CopySnippetBox
              label={
                isRu
                  ? 'Интерактивный установщик с GitHub (одной строкой на чистом VPS)'
                  : 'Interactive GitHub One-Liner (fresh VPS)'
              }
              value="bash <(curl -fsSL https://raw.githubusercontent.com/Velio322/proxy-panel-v4/main/install.sh)"
              isRu={isRu}
            />
            {scenarioTemplates.map((sc) => (
              <div
                key={sc.id}
                className="rack-panel space-y-2.5 rounded-xl border border-border/80 p-3.5"
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="space-y-0.5">
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className="text-[10px]">
                        {sc.badge}
                      </Badge>
                      <span className="text-sm font-semibold">{sc.title}</span>
                    </div>
                    <p className="text-xs text-muted-foreground">{sc.subtitle}</p>
                  </div>
                  <div className="flex items-center gap-1 rounded-md border border-border/70 bg-muted/40 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                    {sc.id === 'both' ? (
                      <Network className="h-3 w-3 text-primary" />
                    ) : (
                      <Server className="h-3 w-3 text-primary" />
                    )}
                    <span>{sc.ports}</span>
                  </div>
                </div>
                <CopySnippetBox value={sc.command} isRu={isRu} />
              </div>
            ))}
          </TabsContent>
        </Tabs>

        <DialogFooter>
          <Button type="button" onClick={() => onOpenChange(false)}>
            {isRu ? 'Закрыть' : 'Close'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
