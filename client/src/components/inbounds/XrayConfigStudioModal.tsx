import { useCallback, useEffect, useRef, useState } from 'react';
import {
  AlertCircle,
  Braces,
  CheckCircle2,
  FileCode2,
  RefreshCw,
  Save,
  Send,
  ShieldCheck,
  Sparkles,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
import { JsonEditor } from '@/components/JsonEditor';
import { getApiErrorMessage, inboundsApi } from '@/lib/api';
import { generateRealityKeypair, generateShortId } from '@/lib/core-utils';
import { toast } from '@/hooks/use-toast';
import type { NodeItem, XrayValidationResult } from '@/types';

interface StudioSnippet {
  id: string;
  label: string;
  apply: (current: Record<string, unknown>) => Record<string, unknown>;
}

const STUDIO_SNIPPETS: StudioSnippet[] = [
  {
    id: 'vless-xhttp-reality',
    label: 'VLESS + XHTTP + Reality (fakedns + sockopt)',
    apply: (cur) => {
      const k = generateRealityKeypair();
      const sid = generateShortId(8);
      const inbounds = Array.isArray(cur.inbounds) ? [...(cur.inbounds as unknown[])] : [];
      inbounds.push({
        tag: 'VLESS_XHTTP_REALITY',
        port: 443,
        listen: '0.0.0.0',
        protocol: 'vless',
        settings: { clients: [], decryption: 'none' },
        sniffing: { enabled: true, destOverride: ['http', 'tls', 'quic', 'fakedns'], routeOnly: true },
        streamSettings: {
          network: 'xhttp',
          sockopt: { mark: 0, V6Only: false, tproxy: 'off', tcpMptcp: false, interface: '' },
          security: 'reality',
          realitySettings: {
            target: 'www.microsoft.com:443',
            privateKey: k.privateKey,
            publicKey: k.publicKey,
            serverNames: ['www.microsoft.com'],
            shortIds: [sid],
            spiderX: '/',
          },
          xhttpSettings: {
            path: '/xh',
            mode: 'packet-up',
            scMaxEachPostBytes: 1000000,
            scMinPostsIntervalMs: 30,
            extra: { xmux: { maxConcurrency: '16-32', hMaxRequestTimes: '600-900' } },
          },
        },
      });
      return {
        log: cur.log ?? { loglevel: 'none' },
        ...cur,
        inbounds,
        outbounds: cur.outbounds ?? [
          { tag: 'direct', protocol: 'freedom' },
          { tag: 'block', protocol: 'blackhole' },
        ],
      };
    },
  },
  {
    id: 'cascade-outbound',
    label: 'Add Kaskad (Cascade Upstream VLESS Outbound)',
    apply: (cur) => {
      const outbounds = Array.isArray(cur.outbounds) ? [...(cur.outbounds as unknown[])] : [];
      outbounds.unshift({
        tag: 'kaskad-upstream',
        protocol: 'vless',
        settings: {
          vnext: [
            {
              address: 'exit.example.com',
              port: 443,
              users: [{ id: '550e8400-e29b-41d4-a716-446655440000', encryption: 'none', flow: 'xtls-rprx-vision' }],
            },
          ],
        },
        streamSettings: {
          network: 'tcp',
          security: 'reality',
          realitySettings: {
            serverName: 'www.apple.com',
            fingerprint: 'chrome',
            publicKey: 'REPLACE_EXIT_PUBLIC_KEY',
            shortId: 'abcd1234',
          },
        },
      });
      const routing = (cur.routing && typeof cur.routing === 'object' ? { ...(cur.routing as Record<string, unknown>) } : {}) as Record<string, unknown>;
      const rules = Array.isArray(routing.rules) ? [...(routing.rules as unknown[])] : [];
      rules.push({
        type: 'field',
        inboundTag: ['VLESS_XHTTP_REALITY'],
        outboundTag: 'kaskad-upstream',
      });
      return { ...cur, outbounds, routing: { domainStrategy: 'IPIfNonMatch', ...routing, rules } };
    },
  },
  {
    id: 'vless-vision-reality',
    label: 'VLESS + Vision + Reality (TCP)',
    apply: (cur) => {
      const k = generateRealityKeypair();
      const inbounds = Array.isArray(cur.inbounds) ? [...(cur.inbounds as unknown[])] : [];
      inbounds.push({
        tag: 'VLESS_VISION',
        port: 443,
        listen: '0.0.0.0',
        protocol: 'vless',
        settings: { clients: [], decryption: 'none' },
        sniffing: { enabled: true, destOverride: ['http', 'tls', 'quic', 'fakedns'], routeOnly: true },
        streamSettings: {
          network: 'tcp',
          security: 'reality',
          realitySettings: {
            target: 'www.google.com:443',
            privateKey: k.privateKey,
            publicKey: k.publicKey,
            serverNames: ['www.google.com'],
            shortIds: [generateShortId(8)],
          },
        },
      });
      return { ...cur, inbounds };
    },
  },
  {
    id: 'dns-fakedns',
    label: 'Insert DNS + FakeDNS Block',
    apply: (cur) => ({
      ...cur,
      dns: {
        servers: [
          { address: 'fakedns', domains: ['geosite:geolocation-!cn'] },
          'https+local://1.1.1.1/dns-query',
        ],
      },
      fakedns: [{ ipPool: '198.18.0.0/15', poolSize: 65535 }],
    }),
  },
  {
    id: 'warp-outbound',
    label: 'Insert WARP / SOCKS5 Exit Outbound',
    apply: (cur) => {
      const outbounds = Array.isArray(cur.outbounds) ? [...(cur.outbounds as unknown[])] : [];
      outbounds.push({
        tag: 'warp-out',
        protocol: 'socks',
        settings: { servers: [{ address: '127.0.0.1', port: 40000 }] },
      });
      return { ...cur, outbounds };
    },
  },
  {
    id: 'sockopt-bbr-mptcp',
    label: 'Enable sockopt MPTCP + TFO + BBR on Inbounds',
    apply: (cur) => {
      const inbounds = Array.isArray(cur.inbounds)
        ? (cur.inbounds as Array<Record<string, unknown>>).map((ib) => ({
            ...ib,
            streamSettings: {
              ...(typeof ib.streamSettings === 'object' && ib.streamSettings ? (ib.streamSettings as Record<string, unknown>) : {}),
              sockopt: {
                mark: 0,
                V6Only: false,
                tproxy: 'off',
                tcpFastOpen: true,
                tcpMptcp: true,
                tcpcongestion: 'bbr',
              },
            },
          }))
        : [];
      return { ...cur, inbounds };
    },
  },
];

export function XrayConfigStudioPanel({
  nodeId,
  nodeName,
  onSaved,
}: {
  nodeId: string;
  nodeName?: string;
  onSaved?: () => void;
}) {
  const [profileName, setProfileName] = useState('XHTTP Kaskad Profile');
  const [jsonText, setJsonText] = useState('{\n  "log": { "loglevel": "warning" },\n  "inbounds": [],\n  "outbounds": []\n}');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [validating, setValidating] = useState(false);
  const [syncInbounds, setSyncInbounds] = useState(false);
  const [selectedSnippet, setSelectedSnippet] = useState('choose');
  const [validation, setValidation] = useState<XrayValidationResult | null>(null);

  const loadSeqRef = useRef(0);
  const validateSeqRef = useRef(0);

  const loadProfile = useCallback(async () => {
    if (!nodeId) return;
    const seq = ++loadSeqRef.current;
    setLoading(true);
    try {
      const res = await inboundsApi.getXrayProfile(nodeId);
      if (seq !== loadSeqRef.current) return;
      setProfileName(res.profileName || `${res.nodeName} Profile`);
      const formatted = JSON.stringify(res.config, null, 2);
      setJsonText(formatted);
    } catch (err) {
      if (seq !== loadSeqRef.current) return;
      toast({ title: 'Failed to load Xray profile', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      if (seq === loadSeqRef.current) {
        setLoading(false);
      }
    }
  }, [nodeId]);

  useEffect(() => {
    void loadProfile();
  }, [loadProfile]);

  const runValidate = useCallback(async (raw: string) => {
    const seq = ++validateSeqRef.current;
    try {
      const parsed = JSON.parse(raw);
      setValidating(true);
      const res = await inboundsApi.validate(parsed);
      if (seq === validateSeqRef.current) {
        setValidation(res);
      }
    } catch (err) {
      if (seq === validateSeqRef.current) {
        setValidation({
          valid: false,
          timestamp: new Date().toTimeString().slice(0, 8),
          message: err instanceof Error ? err.message : 'Invalid JSON syntax',
          errors: [err instanceof Error ? err.message : 'Invalid JSON syntax'],
          warnings: [],
          engine: 'schema-validator',
        });
      }
    } finally {
      if (seq === validateSeqRef.current) {
        setValidating(false);
      }
    }
  }, []);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      if (jsonText.trim()) {
        void runValidate(jsonText);
      }
    }, 550);
    return () => window.clearTimeout(handle);
  }, [jsonText, runValidate]);

  const handleFormat = () => {
    try {
      setJsonText(JSON.stringify(JSON.parse(jsonText), null, 2));
    } catch {
      toast({ title: 'Cannot format invalid JSON', variant: 'destructive' });
    }
  };

  const handleApplySnippet = (snippetId: string) => {
    setSelectedSnippet('choose');
    const sn = STUDIO_SNIPPETS.find((s) => s.id === snippetId);
    if (!sn) return;
    try {
      const parsed = JSON.parse(jsonText) as Record<string, unknown>;
      const next = sn.apply(parsed);
      setJsonText(JSON.stringify(next, null, 2));
      toast({ title: `Inserted: ${sn.label}`, variant: 'success' });
    } catch {
      toast({ title: 'Fix JSON syntax before inserting snippet', variant: 'destructive' });
    }
  };

  const handleSave = async (pushAfterSave: boolean) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(jsonText);
    } catch (err) {
      toast({ title: 'Invalid JSON', description: err instanceof Error ? err.message : 'Syntax error', variant: 'destructive' });
      return;
    }
    setSaving(true);
    try {
      const res = await inboundsApi.saveXrayProfile(nodeId, {
        profileName: profileName.trim() || undefined,
        config: parsed,
        push: pushAfterSave,
        syncInbounds,
      });
      setValidation(res.validation);
      toast({
        title: pushAfterSave ? 'Xray Profile saved & pushed to node' : 'Xray Profile saved',
        variant: 'success',
      });
      onSaved?.();
    } catch (err) {
      toast({ title: 'Save failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-muted/20 p-3">
        <div className="flex items-center gap-3 min-w-0 flex-1">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-primary/30 bg-primary/10 text-primary">
            <FileCode2 className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1 space-y-0.5">
            <Input
              aria-label="Profile Name"
              value={profileName}
              onChange={(e) => setProfileName(e.target.value)}
              className="h-7 border-transparent bg-transparent px-1 font-semibold text-sm hover:border-border focus:border-primary"
            />
            <div className="font-mono text-[11px] text-muted-foreground px-1 break-words min-w-0">
              {nodeName ? `${nodeName} · ` : ''}{nodeId}
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {validation?.xrayVersion && (
            <Badge variant="outline" className="font-mono text-[11px]">
              {validation.xrayVersion.split(' ').slice(0, 2).join(' ')}
            </Badge>
          )}
          <Button type="button" variant="outline" size="sm" onClick={() => void loadProfile()} loading={loading}>
            <RefreshCw className="h-3.5 w-3.5" /> Reload
          </Button>
        </div>
      </div>

      <JsonEditor
        value={jsonText}
        onChange={setJsonText}
        language="json"
        height="420px"
        ariaLabel="Xray Config Studio JSON Editor"
      />

      {/* Live validation status bar */}
      <div
        className={
          validation?.valid !== false
            ? 'flex flex-col gap-1 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3.5 py-2.5 font-mono text-xs text-emerald-400'
            : 'flex flex-col gap-1 rounded-lg border border-red-500/40 bg-red-500/10 px-3.5 py-2.5 font-mono text-xs text-red-400'
        }
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            {validation?.valid !== false ? (
              <CheckCircle2 className="h-4 w-4 shrink-0" />
            ) : (
              <AlertCircle className="h-4 w-4 shrink-0" />
            )}
            <span>
              {validation
                ? `${validation.timestamp} | ${validation.message}`
                : 'Validating Xray configuration...'}
            </span>
          </div>
          {validation?.engine && (
            <span className="text-[11px] opacity-75">[{validation.engine}]</span>
          )}
        </div>
        {validation && !validation.valid && validation.errors.slice(1, 5).map((err, i) => (
          <div key={i} className="pl-6 text-[11px] opacity-90">
            • {err}
          </div>
        ))}
        {validation?.warnings?.map((w, i) => (
          <div key={i} className="pl-6 text-[11px] text-amber-400">
            Warning: {w}
          </div>
        ))}
      </div>

      {/* Bottom action toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" onClick={() => void handleSave(false)} loading={saving}>
            <Save className="h-4 w-4" /> Save
          </Button>
          <Button type="button" variant="secondary" onClick={() => void handleSave(true)} loading={saving}>
            <Send className="h-4 w-4" /> Save &amp; Push
          </Button>
          <Button type="button" variant="outline" onClick={handleFormat}>
            <Sparkles className="h-3.5 w-3.5" /> Format
          </Button>
          <Button type="button" variant="outline" onClick={() => void runValidate(jsonText)} loading={validating}>
            <ShieldCheck className="h-3.5 w-3.5" /> Validate
          </Button>
          <div className="w-60">
            <Select value={selectedSnippet} onValueChange={handleApplySnippet}>
              <SelectTrigger className="h-9 text-xs">
                <Braces className="h-3.5 w-3.5 mr-1 text-primary" />
                <SelectValue placeholder="{ } Snippets" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="choose" disabled>
                  &#123; &#125; Insert Xray Snippet...
                </SelectItem>
                {STUDIO_SNIPPETS.map((sn) => (
                  <SelectItem key={sn.id} value={sn.id}>
                    {sn.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <label className="inline-flex items-center gap-2 text-xs text-muted-foreground cursor-pointer">
          <Switch checked={syncInbounds} onCheckedChange={setSyncInbounds} />
          <Label className="text-xs cursor-pointer">Sync inbound edits to DB</Label>
        </label>
      </div>
    </div>
  );
}

export function XrayConfigStudioModal({
  open,
  onOpenChange,
  nodes,
  initialNodeId,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  nodes: NodeItem[];
  initialNodeId?: string;
  onSaved?: () => void;
}) {
  const [selectedNodeId, setSelectedNodeId] = useState(initialNodeId || nodes[0]?.id || '');

  useEffect(() => {
    if (!open) return;
    if (initialNodeId) {
      setSelectedNodeId(initialNodeId);
    } else if (!selectedNodeId && nodes[0]?.id) {
      setSelectedNodeId(nodes[0].id);
    }
  }, [open, initialNodeId, nodes, selectedNodeId]);

  const activeNode = nodes.find((n) => n.id === selectedNodeId);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="5xl">
        <DialogHeader>
          <div className="flex flex-wrap items-center justify-between gap-3 pr-8">
            <div>
              <DialogTitle>Xray Config Profile Studio</DialogTitle>
              <DialogDescription>
                Full Xray JSON editor with live validation, cascade outbounds, FakeDNS &amp; sockopt.
              </DialogDescription>
            </div>
            {nodes.length > 0 && (
              <div className="w-64">
                <Select value={selectedNodeId || nodes[0].id} onValueChange={setSelectedNodeId}>
                  <SelectTrigger>
                    <SelectValue placeholder="Select Node" />
                  </SelectTrigger>
                  <SelectContent>
                    {nodes.map((n) => (
                      <SelectItem key={n.id} value={n.id}>
                        {n.name} ({n.host})
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
          </div>
        </DialogHeader>

        {selectedNodeId ? (
          <XrayConfigStudioPanel
            key={selectedNodeId}
            nodeId={selectedNodeId}
            nodeName={activeNode?.name}
            onSaved={onSaved}
          />
        ) : (
          <div className="py-12 text-center text-sm text-muted-foreground">
            Create or connect a node first to manage its Xray Config Profile.
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
