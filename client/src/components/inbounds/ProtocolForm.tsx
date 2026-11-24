import { useState } from 'react';
import { Shield, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { Badge } from '@/components/ui/badge';
import { Checkbox } from '@/components/ui/checkbox';
import { getApiErrorMessage, inboundsApi } from '@/lib/api';
import { generateSs2022Key, parsePortRange } from '@/lib/core-utils';
import { toast } from '@/hooks/use-toast';
import type { InboundFormState, NetworkType, VlessFlowType } from '@/components/inbounds/types';

interface ProtocolFormProps {
  form: InboundFormState;
  onChange: (patch: Partial<InboundFormState>) => void;
}

const SNIFFING_DEST_TOKENS = ['http', 'tls', 'quic', 'fakedns'] as const;

export function ProtocolForm({ form, onChange }: ProtocolFormProps) {
  const [keygenLoading, setKeygenLoading] = useState<'vless-x25519' | 'vless-pq' | 'ss2022' | null>(null);
  const isXray =
    form.protocol === 'VLESS' ||
    form.protocol === 'VMESS' ||
    form.protocol === 'TROJAN' ||
    form.protocol === 'SHADOWSOCKS';

  const portRangeValidation = parsePortRange(form.hy2PortRange);

  const handleGenVlessEnc = async (variant: 'x25519' | 'mlkem768') => {
    setKeygenLoading(variant === 'x25519' ? 'vless-x25519' : 'vless-pq');
    try {
      const res = await inboundsApi.generateKeys({ kind: 'vlessenc' });
      const pair = res[variant] as { decryption?: string; encryption?: string } | undefined;
      if (pair?.decryption && pair?.encryption) {
        onChange({ vlessDecryption: pair.decryption, vlessEncryption: pair.encryption });
        toast({ title: `Generated ${variant.toUpperCase()} VLESS Encryption`, variant: 'success' });
      }
    } catch (err) {
      toast({ title: 'Key generation failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setKeygenLoading(null);
    }
  };

  const toggleDestOverride = (token: string) => {
    const next = form.sniffingDestOverride.includes(token)
      ? form.sniffingDestOverride.filter((t) => t !== token)
      : [...form.sniffingDestOverride, token];
    onChange({ sniffingDestOverride: next });
  };

  return (
    <div className="space-y-4">
      {form.protocol === 'VLESS' && (
        <div className="space-y-3">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>XTLS Flow</Label>
              <Select
                value={form.vlessFlow}
                onValueChange={(v) => onChange({ vlessFlow: v as VlessFlowType })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">None</SelectItem>
                  <SelectItem value="xtls-rprx-vision">xtls-rprx-vision</SelectItem>
                  <SelectItem value="xtls-rprx-vision-udp443">xtls-rprx-vision-udp443</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>VLESS Route Tag</Label>
              <Input
                placeholder="e.g. 0x0001"
                value={form.vlessRoute}
                onChange={(e) => onChange({ vlessRoute: e.target.value })}
              />
            </div>
            <div className="sm:col-span-2 space-y-1.5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <Label>Decryption (vlessenc / ML-KEM-768 PQ)</Label>
                <div className="flex flex-wrap gap-1.5">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    loading={keygenLoading === 'vless-x25519'}
                    onClick={() => handleGenVlessEnc('x25519')}
                  >
                    <Sparkles className="h-3.5 w-3.5" /> Gen X25519 Enc
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    loading={keygenLoading === 'vless-pq'}
                    onClick={() => handleGenVlessEnc('mlkem768')}
                  >
                    <Shield className="h-3.5 w-3.5" /> Gen ML-KEM-768 PQ
                  </Button>
                </div>
              </div>
              <Input
                placeholder="none or mlkem768x25519plus.native..."
                value={form.vlessDecryption}
                onChange={(e) => onChange({ vlessDecryption: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
            <div className="sm:col-span-2 space-y-1.5">
              <Label>Client Encryption (vlessenc)</Label>
              <Input
                placeholder="Optional client encryption string (auto-filled by generator)"
                value={form.vlessEncryption}
                onChange={(e) => onChange({ vlessEncryption: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
          </div>
          <details className="rounded-lg border bg-muted/20 p-2.5 text-xs">
            <summary className="cursor-pointer font-medium text-muted-foreground select-none">
              VLESS Fallbacks (JSON array, optional)
            </summary>
            <div className="mt-2">
              <Textarea
                rows={2}
                placeholder='[{"dest": 8080, "xver": 1}]'
                value={form.vlessFallbacksJson}
                onChange={(e) => onChange({ vlessFallbacksJson: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
          </details>
        </div>
      )}

      {form.protocol === 'VMESS' && (
        <div className="grid gap-2 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>VMess Cipher</Label>
            <Select
              value={form.vmessCipher}
              onValueChange={(v) => onChange({ vmessCipher: v as InboundFormState['vmessCipher'] })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">auto</SelectItem>
                <SelectItem value="aes-128-gcm">aes-128-gcm</SelectItem>
                <SelectItem value="chacha20-poly1305">chacha20-poly1305</SelectItem>
                <SelectItem value="none">none</SelectItem>
                <SelectItem value="zero">zero</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Alter ID</Label>
            <Input
              type="number"
              min={0}
              value={form.alterId}
              onChange={(e) => onChange({ alterId: e.target.value })}
            />
          </div>
        </div>
      )}

      {form.protocol === 'TROJAN' && (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label>Fallback Server Password (optional)</Label>
            <Input
              value={form.password}
              onChange={(e) => onChange({ password: e.target.value })}
              placeholder="Per-client credentials are injected automatically"
            />
          </div>
          <div className="space-y-1.5">
            <Label>Trojan Fallbacks (JSON array, optional)</Label>
            <Textarea
              rows={2}
              placeholder='[{"dest": 8080, "xver": 1}]'
              value={form.trojanFallbacksJson}
              onChange={(e) => onChange({ trojanFallbacksJson: e.target.value })}
              className="font-mono text-xs"
            />
          </div>
        </div>
      )}

      {form.protocol === 'SHADOWSOCKS' && (
        <div className="grid gap-2 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>Method</Label>
            <Select
              value={form.ssMethod}
              onValueChange={(v) => onChange({ ssMethod: v as InboundFormState['ssMethod'] })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="2022-blake3-aes-128-gcm">2022-blake3-aes-128-gcm</SelectItem>
                <SelectItem value="2022-blake3-aes-256-gcm">2022-blake3-aes-256-gcm</SelectItem>
                <SelectItem value="2022-blake3-chacha20-poly1305">2022-blake3-chacha20-poly1305</SelectItem>
                <SelectItem value="aes-256-gcm">aes-256-gcm</SelectItem>
                <SelectItem value="aes-128-gcm">aes-128-gcm</SelectItem>
                <SelectItem value="chacha20-ietf-poly1305">chacha20-ietf-poly1305</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Network</Label>
            <Select
              value={form.ssNetwork}
              onValueChange={(v) => onChange({ ssNetwork: v as InboundFormState['ssNetwork'] })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="tcp,udp">tcp,udp</SelectItem>
                <SelectItem value="tcp">tcp</SelectItem>
                <SelectItem value="udp">udp</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="sm:col-span-2 space-y-1.5">
            <div className="flex items-center justify-between">
              <Label>Server PSK / SS-2022 iPSK (Base64)</Label>
              <Button
                type="button"
                variant="outline"
                size="sm"
                loading={keygenLoading === 'ss2022'}
                onClick={async () => {
                  setKeygenLoading('ss2022');
                  try {
                    const res = await inboundsApi.generateKeys({ kind: 'ss2022', method: form.ssMethod });
                    onChange({ password: String(res.key ?? generateSs2022Key(form.ssMethod)) });
                  } catch {
                    onChange({ password: generateSs2022Key(form.ssMethod) });
                  } finally {
                    setKeygenLoading(null);
                  }
                }}
              >
                <Sparkles className="h-3.5 w-3.5" /> Generate Key
              </Button>
            </div>
            <Input
              value={form.password}
              onChange={(e) => onChange({ password: e.target.value })}
              placeholder="Base64 server PSK"
            />
          </div>
        </div>
      )}

      {form.protocol === 'HYSTERIA2' && (
        <div className="space-y-4">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Upload Limit (Mbps)</Label>
              <Input
                type="number"
                min={0}
                placeholder="e.g. 100"
                value={form.hy2BandwidthUp}
                onChange={(e) => onChange({ hy2BandwidthUp: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Download Limit (Mbps)</Label>
              <Input
                type="number"
                min={0}
                placeholder="e.g. 200"
                value={form.hy2BandwidthDown}
                onChange={(e) => onChange({ hy2BandwidthDown: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Max Clients</Label>
              <Input
                type="number"
                min={0}
                value={form.hy2MaxClient}
                onChange={(e) => onChange({ hy2MaxClient: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Max Streams per Client</Label>
              <Input
                type="number"
                min={0}
                value={form.hy2MaxStream}
                onChange={(e) => onChange({ hy2MaxStream: e.target.value })}
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <Label>Port Hopping Range (max 64 ports)</Label>
              {form.hy2PortRange.trim() && (
                <Badge variant={portRangeValidation.valid ? 'success' : 'destructive'}>
                  {portRangeValidation.valid
                    ? `${portRangeValidation.count} / 64 ports`
                    : portRangeValidation.error}
                </Badge>
              )}
            </div>
            <Input
              placeholder="e.g. 20000-20063"
              value={form.hy2PortRange}
              onChange={(e) => onChange({ hy2PortRange: e.target.value })}
            />
          </div>

          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Masquerade (URL or JSON)</Label>
              <Input
                placeholder="https://example.com"
                value={form.hy2Masquerade}
                onChange={(e) => onChange({ hy2Masquerade: e.target.value })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>Ignore Client Bandwidth</Label>
              <Switch
                checked={form.hy2IgnoreClientBandwidth}
                onCheckedChange={(v) => onChange({ hy2IgnoreClientBandwidth: v })}
              />
            </div>
          </div>
        </div>
      )}

      {form.protocol === 'TUIC' && (
        <div className="space-y-3">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Congestion Control</Label>
              <Select
                value={form.tuicCongestionControl}
                onValueChange={(v) =>
                  onChange({ tuicCongestionControl: v as InboundFormState['tuicCongestionControl'] })
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="bbr">bbr</SelectItem>
                  <SelectItem value="cubic">cubic</SelectItem>
                  <SelectItem value="new_reno">new_reno</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>UDP Relay Mode</Label>
              <Select
                value={form.tuicUdpRelayMode}
                onValueChange={(v) =>
                  onChange({ tuicUdpRelayMode: v as 'native' | 'quic' })
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="native">native</SelectItem>
                  <SelectItem value="quic">quic</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>Auth Timeout</Label>
              <Input
                placeholder="3s"
                value={form.tuicAuthTimeout}
                onChange={(e) => onChange({ tuicAuthTimeout: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Heartbeat Interval</Label>
              <Input
                placeholder="10s"
                value={form.tuicHeartbeat}
                onChange={(e) => onChange({ tuicHeartbeat: e.target.value })}
              />
            </div>
          </div>
          <div className="flex items-center justify-between rounded-lg border px-3 py-2">
            <Label>Zero-RTT Handshake</Label>
            <Switch
              checked={form.tuicZeroRtt}
              onCheckedChange={(v) => onChange({ tuicZeroRtt: v })}
            />
          </div>
        </div>
      )}

      {form.protocol === 'NAIVEPROXY' && (
        <div className="grid gap-2 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>Protocol</Label>
            <Select
              value={form.naiveProto}
              onValueChange={(v) => onChange({ naiveProto: v as 'https' | 'quic' })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="https">HTTPS</SelectItem>
                <SelectItem value="quic">QUIC</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Domain (Caddy Site)</Label>
            <Input
              placeholder="naive.example.com"
              value={form.naiveDomain}
              onChange={(e) => onChange({ naiveDomain: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>TLS Mode</Label>
            <Select
              value={form.naiveTlsMode}
              onValueChange={(v) => onChange({ naiveTlsMode: v as 'letsencrypt' | 'custom' })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="custom">Custom Cert/Key</SelectItem>
                <SelectItem value="letsencrypt">Let's Encrypt (ACME)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>ACME Email</Label>
            <Input
              placeholder="admin@example.com"
              value={form.naiveEmail}
              onChange={(e) => onChange({ naiveEmail: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Fallback Root</Label>
            <Input
              value={form.naiveFallbackRoot}
              onChange={(e) => onChange({ naiveFallbackRoot: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>WARP Upstream (optional)</Label>
            <Input
              value={form.naiveWarpUpstream}
              onChange={(e) => onChange({ naiveWarpUpstream: e.target.value })}
            />
          </div>
          <div className="flex items-center justify-between rounded-lg border px-3 py-2">
            <Label>Hide IP</Label>
            <Switch checked={form.naiveHideIp} onCheckedChange={(v) => onChange({ naiveHideIp: v })} />
          </div>
          <div className="flex items-center justify-between rounded-lg border px-3 py-2">
            <Label>Probe Resistance</Label>
            <Switch
              checked={form.naiveProbeResistance}
              onCheckedChange={(v) => onChange({ naiveProbeResistance: v })}
            />
          </div>
        </div>
      )}

      {form.protocol === 'MIERU' && (
        <div className="grid gap-2 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>Transport</Label>
            <Select
              value={form.mieruTransport}
              onValueChange={(v) => onChange({ mieruTransport: v as InboundFormState['mieruTransport'] })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="tcp">TCP</SelectItem>
                <SelectItem value="udp">UDP</SelectItem>
                <SelectItem value="both">Both (TCP + UDP)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Logging Level</Label>
            <Select
              value={form.mieruLoggingLevel}
              onValueChange={(v) =>
                onChange({ mieruLoggingLevel: v as InboundFormState['mieruLoggingLevel'] })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="FATAL">FATAL</SelectItem>
                <SelectItem value="ERROR">ERROR</SelectItem>
                <SelectItem value="WARN">WARN</SelectItem>
                <SelectItem value="INFO">INFO</SelectItem>
                <SelectItem value="DEBUG">DEBUG</SelectItem>
                <SelectItem value="TRACE">TRACE</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Port Range (optional)</Label>
            <Input
              placeholder="e.g. 20000-20020"
              value={form.mieruPortRange}
              onChange={(e) => onChange({ mieruPortRange: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Domain / Host</Label>
            <Input
              value={form.mieruDomain}
              onChange={(e) => onChange({ mieruDomain: e.target.value })}
            />
          </div>
        </div>
      )}

      {form.protocol === 'ANYTLS' && (
        <div className="space-y-3">
          <div className="grid gap-2 sm:grid-cols-3">
            <div className="space-y-1.5">
              <Label>Idle Check Interval</Label>
              <Input
                value={form.anytlsIdleCheckInterval}
                onChange={(e) => onChange({ anytlsIdleCheckInterval: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Idle Timeout</Label>
              <Input
                value={form.anytlsIdleTimeout}
                onChange={(e) => onChange({ anytlsIdleTimeout: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Min Idle Sessions</Label>
              <Input
                type="number"
                min={0}
                value={form.anytlsMinIdleSession}
                onChange={(e) => onChange({ anytlsMinIdleSession: e.target.value })}
              />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label>Padding Scheme (one rule per line)</Label>
            <Textarea
              rows={3}
              placeholder="stop=8&#10;0=30-30&#10;1=100-400"
              value={form.anytlsPaddingScheme}
              onChange={(e) => onChange({ anytlsPaddingScheme: e.target.value })}
              className="font-mono text-xs"
            />
          </div>
        </div>
      )}

      {form.protocol === 'SHADOWTLS' && (
        <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
          <div className="text-xs font-semibold uppercase tracking-wide text-primary">
            ShadowTLS v3 Handshake & Detour (Sing-box)
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>ShadowTLS Version</Label>
              <Select
                value={form.shadowtlsVersion}
                onValueChange={(v) =>
                  onChange({ shadowtlsVersion: v as InboundFormState['shadowtlsVersion'] })
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="3">v3 (Recommended)</SelectItem>
                  <SelectItem value="2">v2</SelectItem>
                  <SelectItem value="1">v1</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>Handshake Server (SNI)</Label>
              <Input
                placeholder="www.microsoft.com"
                value={form.shadowtlsHandshakeServer}
                onChange={(e) =>
                  onChange({ shadowtlsHandshakeServer: e.target.value, shadowtlsSni: e.target.value })
                }
              />
            </div>
            <div className="space-y-1.5">
              <Label>Handshake Port</Label>
              <Input
                type="number"
                min={1}
                max={65535}
                value={form.shadowtlsHandshakePort}
                onChange={(e) => onChange({ shadowtlsHandshakePort: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Detour Target Tag (optional)</Label>
              <Input
                placeholder="e.g. ss-backend"
                value={form.shadowtlsDetour}
                onChange={(e) => onChange({ shadowtlsDetour: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Fallback Password</Label>
              <Input
                placeholder="Per-client password injected automatically"
                value={form.password}
                onChange={(e) => onChange({ password: e.target.value })}
              />
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>Strict Mode (v3)</Label>
              <Switch
                checked={form.shadowtlsStrictMode}
                onCheckedChange={(v) => onChange({ shadowtlsStrictMode: v })}
              />
            </div>
          </div>
        </div>
      )}

      {isXray && (
        <div className="border-t pt-4 space-y-3">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Transport Network</Label>
              <Select
                value={form.network}
                onValueChange={(v) => onChange({ network: v as NetworkType })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="tcp">TCP</SelectItem>
                  <SelectItem value="raw">RAW</SelectItem>
                  <SelectItem value="ws">WebSocket</SelectItem>
                  <SelectItem value="grpc">gRPC</SelectItem>
                  <SelectItem value="httpupgrade">HTTPUpgrade</SelectItem>
                  <SelectItem value="xhttp">XHTTP (SplitHTTP)</SelectItem>
                  <SelectItem value="h2">HTTP/2</SelectItem>
                  <SelectItem value="kcp">mKCP</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {(form.network === 'tcp' || form.network === 'raw') && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>Header Camouflage</Label>
                  <Select
                    value={form.tcpHeaderType}
                    onValueChange={(v) => onChange({ tcpHeaderType: v as 'none' | 'http' })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">None (Clean TCP/RAW)</SelectItem>
                      <SelectItem value="http">HTTP Header Disguise</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>Accept PROXY Protocol</Label>
                  <Switch
                    checked={form.tcpAcceptProxy}
                    onCheckedChange={(v) => onChange({ tcpAcceptProxy: v })}
                  />
                </div>
              </div>
              {form.tcpHeaderType === 'http' && (
                <div className="grid gap-2 sm:grid-cols-2">
                  <div className="space-y-1.5">
                    <Label>HTTP Header Host</Label>
                    <Input
                      placeholder="cdn.example.com"
                      value={form.tcpHeaderHost}
                      onChange={(e) => onChange({ tcpHeaderHost: e.target.value })}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label>HTTP Request Path</Label>
                    <Input
                      placeholder="/"
                      value={form.tcpHeaderPath}
                      onChange={(e) => onChange({ tcpHeaderPath: e.target.value })}
                    />
                  </div>
                </div>
              )}
            </div>
          )}

          {form.network === 'ws' && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>WebSocket Path</Label>
                  <Input value={form.wsPath} onChange={(e) => onChange({ wsPath: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>WebSocket Host</Label>
                  <Input value={form.wsHost} onChange={(e) => onChange({ wsHost: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>Heartbeat Period (s)</Label>
                  <Input
                    type="number"
                    min={0}
                    placeholder="0"
                    value={form.wsHeartbeatPeriod}
                    onChange={(e) => onChange({ wsHeartbeatPeriod: e.target.value })}
                  />
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>Accept PROXY Protocol</Label>
                  <Switch
                    checked={form.wsAcceptProxy}
                    onCheckedChange={(v) => onChange({ wsAcceptProxy: v })}
                  />
                </div>
              </div>
            </div>
          )}

          {form.network === 'grpc' && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>gRPC Service Name</Label>
                  <Input
                    value={form.grpcServiceName}
                    onChange={(e) => onChange({ grpcServiceName: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Authority (optional)</Label>
                  <Input
                    value={form.grpcAuthority}
                    onChange={(e) => onChange({ grpcAuthority: e.target.value })}
                  />
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>Multi Mode</Label>
                  <Switch
                    checked={form.grpcMultiMode}
                    onCheckedChange={(v) => onChange({ grpcMultiMode: v })}
                  />
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>Permit Without Stream</Label>
                  <Switch
                    checked={form.grpcPermitWithoutStream}
                    onCheckedChange={(v) => onChange({ grpcPermitWithoutStream: v })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Idle Timeout (s)</Label>
                  <Input
                    type="number"
                    min={0}
                    placeholder="60"
                    value={form.grpcIdleTimeout}
                    onChange={(e) => onChange({ grpcIdleTimeout: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Initial Window Size (bytes)</Label>
                  <Input
                    type="number"
                    min={0}
                    placeholder="0"
                    value={form.grpcInitialWindowsSize}
                    onChange={(e) => onChange({ grpcInitialWindowsSize: e.target.value })}
                  />
                </div>
              </div>
            </div>
          )}

          {form.network === 'httpupgrade' && (
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>HTTPUpgrade Path</Label>
                <Input
                  value={form.httpupgradePath}
                  onChange={(e) => onChange({ httpupgradePath: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <Label>HTTPUpgrade Host</Label>
                <Input
                  value={form.httpupgradeHost}
                  onChange={(e) => onChange({ httpupgradeHost: e.target.value })}
                />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2 sm:col-span-2">
                <Label>Accept PROXY Protocol</Label>
                <Switch
                  checked={form.httpupgradeAcceptProxy}
                  onCheckedChange={(v) => onChange({ httpupgradeAcceptProxy: v })}
                />
              </div>
            </div>
          )}

          {form.network === 'xhttp' && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
              <div className="grid gap-2 sm:grid-cols-3">
                <div className="space-y-1.5">
                  <Label>XHTTP Path</Label>
                  <Input
                    value={form.xhttpPath}
                    onChange={(e) => onChange({ xhttpPath: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>XHTTP Host</Label>
                  <Input
                    value={form.xhttpHost}
                    onChange={(e) => onChange({ xhttpHost: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>XHTTP Mode</Label>
                  <Select
                    value={form.xhttpMode}
                    onValueChange={(v) =>
                      onChange({ xhttpMode: v as InboundFormState['xhttpMode'] })
                    }
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">auto</SelectItem>
                      <SelectItem value="packet-up">packet-up</SelectItem>
                      <SelectItem value="stream-up">stream-up</SelectItem>
                      <SelectItem value="stream-one">stream-one</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <div className="grid gap-2 sm:grid-cols-3">
                <div className="space-y-1.5">
                  <Label>xmux maxConcurrency</Label>
                  <Input
                    placeholder="e.g. 16-32"
                    value={form.xhttpMaxConcurrency}
                    onChange={(e) => onChange({ xhttpMaxConcurrency: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>xmux maxConnections</Label>
                  <Input
                    placeholder="e.g. 0"
                    value={form.xhttpMaxConnections}
                    onChange={(e) => onChange({ xhttpMaxConnections: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>xmux hMaxRequestTimes</Label>
                  <Input
                    placeholder="e.g. 600-900"
                    value={form.xhttpHMaxRequestTimes}
                    onChange={(e) => onChange({ xhttpHMaxRequestTimes: e.target.value })}
                  />
                </div>
              </div>
              <details className="rounded-md border bg-background/40 p-2.5 text-xs">
                <summary className="cursor-pointer font-medium text-muted-foreground select-none">
                  XHTTP v26 Tuning (xmux reuse, scMaxEachPostBytes, padding, headers, downloadSettings)
                </summary>
                <div className="mt-3 space-y-3">
                  <div className="grid gap-2 sm:grid-cols-3">
                    <div className="space-y-1.5">
                      <Label>cMaxReuseTimes</Label>
                      <Input
                        placeholder="0"
                        value={form.xhttpCMaxReuseTimes}
                        onChange={(e) => onChange({ xhttpCMaxReuseTimes: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>hMaxReusableSecs</Label>
                      <Input
                        placeholder="1800-3000"
                        value={form.xhttpHMaxReusableSecs}
                        onChange={(e) => onChange({ xhttpHMaxReusableSecs: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>hKeepAlivePeriod (s)</Label>
                      <Input
                        placeholder="0"
                        value={form.xhttpHKeepAlivePeriod}
                        onChange={(e) => onChange({ xhttpHKeepAlivePeriod: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>scMaxEachPostBytes</Label>
                      <Input
                        placeholder="1000000"
                        value={form.xhttpScMaxEachPostBytes}
                        onChange={(e) => onChange({ xhttpScMaxEachPostBytes: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>scMinPostsIntervalMs</Label>
                      <Input
                        placeholder="30"
                        value={form.xhttpScMinPostsIntervalMs}
                        onChange={(e) => onChange({ xhttpScMinPostsIntervalMs: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>scMaxBufferedPosts</Label>
                      <Input
                        placeholder="30"
                        value={form.xhttpScMaxBufferedPosts}
                        onChange={(e) => onChange({ xhttpScMaxBufferedPosts: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>scStreamUpServerSecs</Label>
                      <Input
                        placeholder="20-80"
                        value={form.xhttpScStreamUpServerSecs}
                        onChange={(e) => onChange({ xhttpScStreamUpServerSecs: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>xPaddingBytes</Label>
                      <Input
                        placeholder="100-1000"
                        value={form.xhttpXPaddingBytes}
                        onChange={(e) => onChange({ xhttpXPaddingBytes: e.target.value })}
                      />
                    </div>
                  </div>
                  <div className="grid gap-2 sm:grid-cols-2">
                    <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                      <Label>noGRPCHeader</Label>
                      <Switch
                        checked={form.xhttpNoGrpcHeader}
                        onCheckedChange={(v) => onChange({ xhttpNoGrpcHeader: v })}
                      />
                    </div>
                    <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                      <Label>noSSEHeader</Label>
                      <Switch
                        checked={form.xhttpNoSseHeader}
                        onCheckedChange={(v) => onChange({ xhttpNoSseHeader: v })}
                      />
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    <Label>Custom XHTTP Headers (JSON)</Label>
                    <Textarea
                      rows={2}
                      placeholder='{"User-Agent": "Mozilla/5.0"}'
                      value={form.xhttpHeadersJson}
                      onChange={(e) => onChange({ xhttpHeadersJson: e.target.value })}
                      className="font-mono text-xs"
                    />
                  </div>
                </div>
              </details>
              <div className="space-y-1.5">
                <Label>XHTTP downloadSettings (JSON, optional)</Label>
                <Textarea
                  rows={2}
                  placeholder='{"address": "...", "port": 443}'
                  value={form.xhttpDownloadSettingsJson}
                  onChange={(e) => onChange({ xhttpDownloadSettingsJson: e.target.value })}
                  className="font-mono text-xs"
                />
              </div>
            </div>
          )}

          {form.network === 'h2' && (
            <div className="grid gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>H2 Path</Label>
                <Input value={form.h2Path} onChange={(e) => onChange({ h2Path: e.target.value })} />
              </div>
              <div className="space-y-1.5">
                <Label>H2 Hosts (comma-separated)</Label>
                <Input value={form.h2Host} onChange={(e) => onChange({ h2Host: e.target.value })} />
              </div>
            </div>
          )}

          {form.network === 'kcp' && (
            <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>Header Type</Label>
                  <Select
                    value={form.kcpHeaderType}
                    onValueChange={(v) =>
                      onChange({ kcpHeaderType: v as InboundFormState['kcpHeaderType'] })
                    }
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">none</SelectItem>
                      <SelectItem value="srtp">srtp</SelectItem>
                      <SelectItem value="utp">utp</SelectItem>
                      <SelectItem value="wechat-video">wechat-video</SelectItem>
                      <SelectItem value="dtls">dtls</SelectItem>
                      <SelectItem value="wireguard">wireguard</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1.5">
                  <Label>mKCP Seed</Label>
                  <Input value={form.kcpSeed} onChange={(e) => onChange({ kcpSeed: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>MTU</Label>
                  <Input placeholder="1350" value={form.kcpMtu} onChange={(e) => onChange({ kcpMtu: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>TTI (ms)</Label>
                  <Input placeholder="20" value={form.kcpTti} onChange={(e) => onChange({ kcpTti: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>Uplink Capacity (MB/s)</Label>
                  <Input placeholder="5" value={form.kcpUplinkCapacity} onChange={(e) => onChange({ kcpUplinkCapacity: e.target.value })} />
                </div>
                <div className="space-y-1.5">
                  <Label>Downlink Capacity (MB/s)</Label>
                  <Input placeholder="20" value={form.kcpDownlinkCapacity} onChange={(e) => onChange({ kcpDownlinkCapacity: e.target.value })} />
                </div>
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>Enable Congestion Control</Label>
                <Switch
                  checked={form.kcpCongestion}
                  onCheckedChange={(v) => onChange({ kcpCongestion: v })}
                />
              </div>
            </div>
          )}

          {/* Xray 26 Sniffing + Sockopt */}
          <details className="rounded-lg border bg-muted/15 p-3 text-xs">
            <summary className="cursor-pointer font-semibold uppercase tracking-wide text-primary select-none">
              Sniffing &amp; Sockopt (Xray 26)
            </summary>
            <div className="mt-3 space-y-4">
              <div className="space-y-2">
                <Label>sniffing.destOverride</Label>
                <div className="flex flex-wrap gap-3 pt-1">
                  {SNIFFING_DEST_TOKENS.map((tok) => (
                    <label key={tok} className="inline-flex items-center gap-1.5 cursor-pointer">
                      <Checkbox
                        checked={form.sniffingDestOverride.includes(tok)}
                        onCheckedChange={() => toggleDestOverride(tok)}
                      />
                      <span className="font-mono">{tok}</span>
                    </label>
                  ))}
                </div>
                <div className="grid gap-2 pt-2 sm:grid-cols-2">
                  <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                    <Label>routeOnly</Label>
                    <Switch
                      checked={form.sniffingRouteOnly}
                      onCheckedChange={(v) => onChange({ sniffingRouteOnly: v })}
                    />
                  </div>
                  <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                    <Label>metadataOnly</Label>
                    <Switch
                      checked={form.sniffingMetadataOnly}
                      onCheckedChange={(v) => onChange({ sniffingMetadataOnly: v })}
                    />
                  </div>
                </div>
                <div className="space-y-1.5 pt-1">
                  <Label>domainsExcluded (comma-separated)</Label>
                  <Input
                    placeholder="courier.push.apple.com, push.services.mozilla.com"
                    value={form.sniffingDomainsExcluded}
                    onChange={(e) => onChange({ sniffingDomainsExcluded: e.target.value })}
                  />
                </div>
              </div>

              <div className="border-t pt-3 space-y-3">
                <div className="flex items-center justify-between">
                  <Label>Enable streamSettings.sockopt</Label>
                  <Switch
                    checked={form.sockoptEnabled}
                    onCheckedChange={(v) => onChange({ sockoptEnabled: v })}
                  />
                </div>
                {form.sockoptEnabled && (
                  <div className="grid gap-2 sm:grid-cols-3">
                    <div className="space-y-1.5">
                      <Label>SO_MARK</Label>
                      <Input
                        type="number"
                        value={form.sockoptMark}
                        onChange={(e) => onChange({ sockoptMark: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>tproxy</Label>
                      <Select
                        value={form.sockoptTproxy}
                        onValueChange={(v) =>
                          onChange({ sockoptTproxy: v as InboundFormState['sockoptTproxy'] })
                        }
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="off">off</SelectItem>
                          <SelectItem value="tproxy">tproxy</SelectItem>
                          <SelectItem value="redirect">redirect</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                    <div className="space-y-1.5">
                      <Label>tcpCongestion</Label>
                      <Select
                        value={form.sockoptTcpCongestion || 'default'}
                        onValueChange={(v) =>
                          onChange({ sockoptTcpCongestion: v === 'default' ? '' : v })
                        }
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="default">Default</SelectItem>
                          <SelectItem value="bbr">bbr</SelectItem>
                          <SelectItem value="cubic">cubic</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                    <div className="space-y-1.5">
                      <Label>interface</Label>
                      <Input
                        placeholder="eth0"
                        value={form.sockoptInterface}
                        onChange={(e) => onChange({ sockoptInterface: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>tcpKeepAliveIdle (s)</Label>
                      <Input
                        type="number"
                        placeholder="300"
                        value={form.sockoptTcpKeepAliveIdle}
                        onChange={(e) => onChange({ sockoptTcpKeepAliveIdle: e.target.value })}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>tcpKeepAliveInterval (s)</Label>
                      <Input
                        type="number"
                        placeholder="15"
                        value={form.sockoptTcpKeepAliveInterval}
                        onChange={(e) => onChange({ sockoptTcpKeepAliveInterval: e.target.value })}
                      />
                    </div>
                    <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                      <Label>V6Only</Label>
                      <Switch
                        checked={form.sockoptV6Only}
                        onCheckedChange={(v) => onChange({ sockoptV6Only: v })}
                      />
                    </div>
                    <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                      <Label>tcpFastOpen</Label>
                      <Switch
                        checked={form.sockoptTcpFastOpen}
                        onCheckedChange={(v) => onChange({ sockoptTcpFastOpen: v })}
                      />
                    </div>
                    <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                      <Label>tcpMptcp</Label>
                      <Switch
                        checked={form.sockoptTcpMptcp}
                        onCheckedChange={(v) => onChange({ sockoptTcpMptcp: v })}
                      />
                    </div>
                  </div>
                )}
              </div>
            </div>
          </details>
        </div>
      )}
    </div>
  );
}
