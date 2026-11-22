import { useEffect, useState } from 'react';
import { Braces, CheckCircle2, AlertCircle, ShieldCheck } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ProtocolForm } from '@/components/inbounds/ProtocolForm';
import { TabSecurity } from '@/components/inbounds/TabSecurity';
import { PortSharesTab } from '@/components/inbounds/PortSharesTab';
import {
  applyRawInboundJsonToForm,
  createDefaultInboundForm,
  formToPayload,
  inboundToForm,
} from '@/components/inbounds/inboundFormMapper';
import type { InboundFormState } from '@/components/inbounds/types';
import { JsonEditor } from '@/components/JsonEditor';
import { getApiErrorMessage, inboundsApi } from '@/lib/api';
import { PROTOCOLS, generateRealityKeypair, generateShortId, generateSs2022Key, parsePortRange } from '@/lib/core-utils';
import { toast } from '@/hooks/use-toast';
import type { Inbound, NodeItem, Protocol, XrayValidationResult } from '@/types';

interface InboundModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  inbound?: Inbound | null;
  nodes: NodeItem[];
  defaultNodeId?: string;
  onSaved: () => void;
}

const INBOUND_PRESETS: Array<{ id: string; label: string; apply: (f: InboundFormState) => Partial<InboundFormState> }> = [
  {
    id: 'vless-xhttp-reality',
    label: 'VLESS + XHTTP + Reality (v26)',
    apply: () => {
      const k = generateRealityKeypair();
      return {
        protocol: 'VLESS',
        tag: 'VLESS_XHTTP_REALITY',
        port: '443',
        network: 'xhttp',
        security: 'reality',
        vlessFlow: 'none',
        vlessDecryption: 'none',
        alpn: 'h2,http/1.1',
        fingerprint: 'edge',
        xhttpPath: '/',
        xhttpMode: 'packet-up',
        xhttpXPaddingBytes: '100-1000',
        xhttpMaxConcurrency: '16-32',
        xhttpScMaxEachPostBytes: '1000000',
        realityDest: 'www.microsoft.com:443',
        realityServerNames: 'www.microsoft.com',
        realityPrivateKey: k.privateKey,
        realityPublicKey: k.publicKey,
        realityShortIds: generateShortId(8),
        sniffingDestOverride: ['http', 'tls', 'quic', 'fakedns'],
        sockoptEnabled: true,
        sockoptTcpMptcp: true,
      };
    },
  },
  {
    id: 'vless-vision-reality',
    label: 'VLESS + Vision + Reality (Stealth TCP)',
    apply: () => {
      const k = generateRealityKeypair();
      return {
        protocol: 'VLESS',
        tag: 'VLESS_VISION_REALITY',
        port: '443',
        network: 'tcp',
        security: 'reality',
        vlessFlow: 'xtls-rprx-vision',
        vlessDecryption: 'none',
        realityDest: 'www.apple.com:443',
        realityServerNames: 'www.apple.com',
        realityPrivateKey: k.privateKey,
        realityPublicKey: k.publicKey,
        realityShortIds: generateShortId(8),
      };
    },
  },
  {
    id: 'vless-ws-cdn',
    label: 'VLESS + WebSocket + CDN TLS',
    apply: () => ({
      protocol: 'VLESS',
      tag: 'VLESS_WS_CDN',
      port: '2053',
      network: 'ws',
      security: 'tls',
      vlessFlow: 'none',
      wsPath: '/ws',
      alpn: 'http/1.1',
      tlsRejectUnknownSni: false,
    }),
  },
  {
    id: 'vless-grpc-reality',
    label: 'VLESS + gRPC + Reality',
    apply: () => {
      const k = generateRealityKeypair();
      return {
        protocol: 'VLESS',
        tag: 'VLESS_GRPC_REALITY',
        port: '8443',
        network: 'grpc',
        security: 'reality',
        vlessFlow: 'none',
        grpcServiceName: 'grpc-tun',
        grpcMultiMode: true,
        realityDest: 'www.cloudflare.com:443',
        realityServerNames: 'www.cloudflare.com',
        realityPrivateKey: k.privateKey,
        realityPublicKey: k.publicKey,
        realityShortIds: generateShortId(8),
      };
    },
  },
  {
    id: 'hy2-hopping-gecko',
    label: 'Hysteria2 + Port Hopping + BBR',
    apply: () => ({
      protocol: 'HYSTERIA2',
      tag: 'HY2_HOP_BBR',
      port: '443',
      hy2PortRange: '20000-20063',
      hy2BbrProfile: 'aggressive',
      hy2ObfsType: 'gecko',
      hy2ObfsPassword: generateShortId(8),
      disableChromeParrot: true,
    }),
  },
  {
    id: 'tuic-v5-0rtt',
    label: 'TUIC v5 + 0-RTT',
    apply: () => ({
      protocol: 'TUIC',
      tag: 'TUIC_V5_0RTT',
      port: '8443',
      tuicCongestionControl: 'bbr',
      tuicZeroRtt: true,
      tuicAuthTimeout: '3s',
      tuicHeartbeat: '10s',
      tuicUdpRelayMode: 'native',
    }),
  },
  {
    id: 'anytls-padding',
    label: 'AnyTLS + Padding Scheme',
    apply: () => ({
      protocol: 'ANYTLS',
      tag: 'ANYTLS_PAD',
      port: '8443',
      anytlsPaddingScheme: 'stop=8\n0=30-30\n1=100-400',
    }),
  },
  {
    id: 'shadowtls-v3',
    label: 'ShadowTLS v3',
    apply: () => ({
      protocol: 'SHADOWTLS',
      tag: 'SHADOWTLS_V3',
      port: '443',
      shadowtlsVersion: '3',
      shadowtlsHandshakeServer: 'www.microsoft.com',
      shadowtlsHandshakePort: '443',
      shadowtlsStrictMode: true,
    }),
  },
  {
    id: 'ss2022',
    label: 'Shadowsocks-2022 (AES-128)',
    apply: () => ({
      protocol: 'SHADOWSOCKS',
      tag: 'SS2022_AES128',
      port: '8388',
      ssMethod: '2022-blake3-aes-128-gcm',
      ssNetwork: 'tcp,udp',
      password: generateSs2022Key('2022-blake3-aes-128-gcm'),
    }),
  },
];

export function InboundModal({
  open,
  onOpenChange,
  inbound,
  nodes,
  defaultNodeId,
  onSaved,
}: InboundModalProps) {
  const [form, setForm] = useState<InboundFormState>(() =>
    createDefaultInboundForm(defaultNodeId || nodes[0]?.id || ''),
  );
  const [activeTab, setActiveTab] = useState('general');
  const [rawText, setRawText] = useState('');
  const [rawDirty, setRawDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [rawErrors, setRawErrors] = useState<string[]>([]);
  const [validating, setValidating] = useState(false);
  const [validationRes, setValidationRes] = useState<XrayValidationResult | null>(null);

  useEffect(() => {
    if (!open) return;
    const initial = inbound
      ? inboundToForm(inbound)
      : createDefaultInboundForm(defaultNodeId || nodes[0]?.id || '');
    setForm(initial);
    setRawText(JSON.stringify(formToPayload(initial), null, 2));
    setRawDirty(false);
    setValidationRes(null);
    setActiveTab('general');
  }, [open, inbound, defaultNodeId, nodes]);

  const patchForm = (patch: Partial<InboundFormState>) => {
    setForm((prev) => {
      const next = { ...prev, ...patch };
      if (!rawDirty) {
        setRawText(JSON.stringify(formToPayload(next), null, 2));
      }
      return next;
    });
  };

  const handleTabChange = (nextTab: string) => {
    if (activeTab === 'raw' && rawDirty && rawErrors.length === 0) {
      const parsed = applyRawInboundJsonToForm(rawText, form);
      if (!parsed.error) {
        setForm(parsed.form);
        setRawDirty(false);
      }
    } else if (nextTab === 'raw' && !rawDirty) {
      setRawText(JSON.stringify(formToPayload(form), null, 2));
    }
    setActiveTab(nextTab);
  };

  const handleApplyRawToForm = () => {
    const res = applyRawInboundJsonToForm(rawText, form);
    if (res.error) {
      toast({ title: 'Invalid JSON', description: res.error, variant: 'destructive' });
      return;
    }
    setForm(res.form);
    setRawDirty(false);
    toast({ title: 'JSON synced to form fields', variant: 'success' });
  };

  const handleServerValidate = async () => {
    let target: unknown = formToPayload(form);
    if (rawDirty) {
      try {
        target = JSON.parse(rawText);
      } catch (e) {
        setValidationRes({
          valid: false,
          timestamp: new Date().toTimeString().slice(0, 8),
          message: e instanceof Error ? e.message : 'Invalid JSON',
          errors: [e instanceof Error ? e.message : 'Invalid JSON'],
          warnings: [],
          engine: 'schema-validator',
        });
        return;
      }
    }
    setValidating(true);
    try {
      const res = await inboundsApi.validate(target);
      setValidationRes(res);
    } catch (err) {
      toast({ title: 'Validation failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setValidating(false);
    }
  };

  const handleSave = async () => {
    let effectiveForm = form;
    if (activeTab === 'raw' || rawDirty) {
      const synced = applyRawInboundJsonToForm(rawText, form);
      if (synced.error) {
        toast({ title: 'Invalid Raw JSON', description: synced.error, variant: 'destructive' });
        return;
      }
      effectiveForm = synced.form;
      setForm(effectiveForm);
    }

    if (!effectiveForm.nodeId) {
      toast({ title: 'Node is required', variant: 'destructive' });
      return;
    }
    if (!effectiveForm.tag.trim()) {
      toast({ title: 'Tag is required', variant: 'destructive' });
      return;
    }
    if (effectiveForm.protocol === 'HYSTERIA2' && effectiveForm.hy2PortRange.trim()) {
      const pr = parsePortRange(effectiveForm.hy2PortRange);
      if (!pr.valid) {
        toast({ title: 'Invalid port hopping range', description: pr.error, variant: 'destructive' });
        return;
      }
    }

    setSaving(true);
    try {
      const payload = rawDirty ? (JSON.parse(rawText) as Record<string, unknown>) : formToPayload(effectiveForm);
      if (inbound) {
        await inboundsApi.update(inbound.id, payload);
        // Persist PortShares diff on edit
        const existingShares = inbound.portShares ?? [];
        const currentIds = new Set(effectiveForm.portShares.map((s) => s.id).filter(Boolean));
        for (const oldShare of existingShares) {
          if (oldShare.id && !currentIds.has(oldShare.id)) {
            await inboundsApi.deletePortShare(inbound.id, oldShare.id);
          }
        }
        for (const share of effectiveForm.portShares) {
          const shareBody = {
            protocol: share.protocol,
            tag: share.tag,
            host: share.host || null,
            path: share.path || null,
            enable: share.enable,
            settings: share.settings,
            stream: share.stream,
          };
          if (share.id) {
            await inboundsApi.updatePortShare(inbound.id, share.id, shareBody);
          } else {
            await inboundsApi.createPortShare(inbound.id, shareBody);
          }
        }
        toast({ title: 'Inbound updated', variant: 'success' });
      } else {
        const created = await inboundsApi.create(payload);
        for (const share of effectiveForm.portShares) {
          await inboundsApi.createPortShare(created.id, {
            protocol: share.protocol,
            tag: share.tag,
            host: share.host || null,
            path: share.path || null,
            enable: share.enable,
            settings: share.settings,
            stream: share.stream,
          });
        }
        toast({ title: 'Inbound created', variant: 'success' });
      }
      onSaved();
      onOpenChange(false);
    } catch (err) {
      toast({ title: 'Save failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex flex-wrap items-center justify-between gap-2 pr-6">
            <div>
              <DialogTitle>{inbound ? `Edit Inbound: ${inbound.tag}` : 'Create Inbound'}</DialogTitle>
              <DialogDescription>
                Configure protocol, transport, TLS/Reality security, routing, and PortShares.
              </DialogDescription>
            </div>
            <div className="w-56">
              <Select
                value="preset"
                onValueChange={(id) => {
                  const found = INBOUND_PRESETS.find((p) => p.id === id);
                  if (!found) return;
                  setRawDirty(false);
                  const nextForm: InboundFormState = { ...form, ...found.apply(form) };
                  setForm(nextForm);
                  setRawText(JSON.stringify(formToPayload(nextForm), null, 2));
                  toast({ title: `Preset applied: ${found.label}`, variant: 'success' });
                }}
              >
                <SelectTrigger className="h-8 text-xs">
                  <Braces className="h-3.5 w-3.5 mr-1 text-primary" />
                  <SelectValue placeholder="Apply Preset..." />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="preset" disabled>
                    Load Protocol Preset...
                  </SelectItem>
                  {INBOUND_PRESETS.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </DialogHeader>

        <Tabs value={activeTab} onValueChange={handleTabChange} className="w-full">
          <TabsList className="w-full justify-start flex-wrap">
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="protocol">Protocol & Transport</TabsTrigger>
            <TabsTrigger value="security">Security</TabsTrigger>
            <TabsTrigger value="routing">Routing</TabsTrigger>
            <TabsTrigger value="shares">PortShares ({form.portShares.length})</TabsTrigger>
            <TabsTrigger value="raw">Raw JSON</TabsTrigger>
          </TabsList>

          <TabsContent value="general" className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>Node</Label>
                <Select
                  value={form.nodeId || 'unassigned'}
                  onValueChange={(v) => patchForm({ nodeId: v === 'unassigned' ? '' : v })}
                  disabled={Boolean(inbound)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select node" />
                  </SelectTrigger>
                  <SelectContent>
                    {nodes.length === 0 && <SelectItem value="unassigned">No nodes</SelectItem>}
                    {nodes.map((n) => (
                      <SelectItem key={n.id} value={n.id}>
                        {n.name} ({n.host})
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>Protocol</Label>
                <Select
                  value={form.protocol}
                  onValueChange={(v) => patchForm({ protocol: v as Protocol })}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PROTOCOLS.map((p) => (
                      <SelectItem key={p} value={p}>
                        {p}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>Tag</Label>
                <Input value={form.tag} onChange={(e) => patchForm({ tag: e.target.value })} />
              </div>
              <div className="space-y-1.5">
                <Label>Port</Label>
                <Input
                  type="number"
                  min={1}
                  max={65535}
                  value={form.port}
                  onChange={(e) => patchForm({ port: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <Label>Listen Address</Label>
                <Input value={form.listen} onChange={(e) => patchForm({ listen: e.target.value })} />
              </div>
              <div className="space-y-1.5">
                <Label>Remark</Label>
                <Input
                  placeholder="Optional remark"
                  value={form.remark}
                  onChange={(e) => patchForm({ remark: e.target.value })}
                />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>Enable Inbound</Label>
                <Switch checked={form.enable} onCheckedChange={(v) => patchForm({ enable: v })} />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>Sniffing</Label>
                <Switch checked={form.sniffing} onCheckedChange={(v) => patchForm({ sniffing: v })} />
              </div>
            </div>
          </TabsContent>

          <TabsContent value="protocol">
            <ProtocolForm form={form} onChange={patchForm} />
          </TabsContent>

          <TabsContent value="security">
            <TabSecurity form={form} onChange={patchForm} />
          </TabsContent>

          <TabsContent value="routing" className="space-y-3">
            <div className="flex items-center justify-between rounded-lg border px-3 py-2.5">
              <div>
                <div className="text-sm font-medium">Block BitTorrent</div>
                <div className="text-xs text-muted-foreground">Drop P2P / BitTorrent traffic on this inbound</div>
              </div>
              <Switch
                checked={form.routingBlockTorrent}
                onCheckedChange={(v) => patchForm({ routingBlockTorrent: v })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2.5">
              <div>
                <div className="text-sm font-medium">Block Ads</div>
                <div className="text-xs text-muted-foreground">Block known advertising domains</div>
              </div>
              <Switch
                checked={form.routingBlockAds}
                onCheckedChange={(v) => patchForm({ routingBlockAds: v })}
              />
            </div>
          </TabsContent>

          <TabsContent value="shares">
            <PortSharesTab
              shares={form.portShares}
              onChange={(portShares) => patchForm({ portShares })}
            />
          </TabsContent>

          <TabsContent value="raw" className="space-y-3">
            <JsonEditor
              value={rawText}
              onChange={(next) => {
                setRawText(next);
                setRawDirty(true);
              }}
              language="json"
              schemaKind="xray-inbound"
              onValidationChange={setRawErrors}
              height="300px"
            />
            {validationRes && (
              <div
                className={
                  validationRes.valid
                    ? 'flex items-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 font-mono text-xs text-emerald-400'
                    : 'flex flex-col gap-1 rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 font-mono text-xs text-red-400'
                }
              >
                <div className="flex items-center gap-2">
                  {validationRes.valid ? (
                    <CheckCircle2 className="h-4 w-4 shrink-0" />
                  ) : (
                    <AlertCircle className="h-4 w-4 shrink-0" />
                  )}
                  <span>
                    {validationRes.timestamp} | {validationRes.message}
                  </span>
                </div>
                {validationRes.warnings.map((w, idx) => (
                  <div key={idx} className="text-[11px] text-amber-400 pl-6">
                    Warning: {w}
                  </div>
                ))}
              </div>
            )}
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleServerValidate}
                  loading={validating}
                >
                  <ShieldCheck className="h-3.5 w-3.5" /> Validate Config
                </Button>
                {rawDirty && (
                  <Button type="button" variant="secondary" size="sm" onClick={handleApplyRawToForm}>
                    Sync JSON to Form
                  </Button>
                )}
              </div>
              <span className="text-[11px] text-muted-foreground">
                Editable Raw JSON synced with all tabs
              </span>
            </div>
          </TabsContent>
        </Tabs>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            onClick={handleSave}
            loading={saving}
            disabled={rawErrors.length > 0}
          >
            {inbound ? 'Save Changes' : 'Create Inbound'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
