import { useState } from 'react';
import { KeyRound, RefreshCw, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { getApiErrorMessage, inboundsApi } from '@/lib/api';
import { generateRealityKeypair, generateShortId } from '@/lib/core-utils';
import { toast } from '@/hooks/use-toast';
import type { Hy2BbrProfile, Hy2ObfsType, InboundFormState, SecurityType } from '@/components/inbounds/types';

interface TabSecurityProps {
  form: InboundFormState;
  onChange: (patch: Partial<InboundFormState>) => void;
}

export function TabSecurity({ form, onChange }: TabSecurityProps) {
  const [genBusy, setGenBusy] = useState<'reality' | 'mldsa65' | null>(null);
  const isXray =
    form.protocol === 'VLESS' ||
    form.protocol === 'VMESS' ||
    form.protocol === 'TROJAN' ||
    form.protocol === 'SHADOWSOCKS';

  const handleGenReality = async () => {
    setGenBusy('reality');
    try {
      const res = await inboundsApi.generateKeys({ kind: 'reality' });
      onChange({
        realityPrivateKey: String(res.privateKey ?? ''),
        realityPublicKey: String(res.publicKey ?? ''),
        ...(typeof res.shortId === 'string' && !form.realityShortIds.trim()
          ? { realityShortIds: res.shortId }
          : {}),
      });
    } catch {
      const pair = generateRealityKeypair();
      onChange({
        realityPrivateKey: pair.privateKey,
        realityPublicKey: pair.publicKey,
      });
    } finally {
      setGenBusy(null);
    }
  };

  const handleGenMldsa65 = async () => {
    setGenBusy('mldsa65');
    try {
      const res = await inboundsApi.generateKeys({ kind: 'mldsa65' });
      onChange({
        mldsa65Seed: String(res.seed ?? ''),
        mldsa65Verify: String(res.verify ?? ''),
      });
      toast({ title: 'ML-DSA-65 post-quantum keys generated', variant: 'success' });
    } catch (err) {
      toast({ title: 'ML-DSA-65 generation failed', description: getApiErrorMessage(err), variant: 'destructive' });
    } finally {
      setGenBusy(null);
    }
  };

  return (
    <div className="space-y-4">
      {isXray && (
        <div className="space-y-1.5">
          <Label>Stream Security</Label>
          <Select
            value={form.security}
            onValueChange={(v) => onChange({ security: v as SecurityType })}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="none">None</SelectItem>
              <SelectItem value="tls">TLS</SelectItem>
              <SelectItem value="reality">Reality</SelectItem>
            </SelectContent>
          </Select>
        </div>
      )}

      {/* Common SNI / TLS fields for TLS & non-Xray TLS-capable protocols */}
      {(form.security === 'tls' || !isXray) && (
        <div className="space-y-3 rounded-lg border p-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>SNI (Server Name)</Label>
              <Input
                placeholder="example.com"
                value={form.sni}
                onChange={(e) => onChange({ sni: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>ALPN (comma-separated)</Label>
              <Input
                placeholder="h2,http/1.1"
                value={form.alpn}
                onChange={(e) => onChange({ alpn: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Certificate Path</Label>
              <Input
                placeholder="/etc/ssl/cert.pem"
                value={form.certFile}
                onChange={(e) => onChange({ certFile: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Private Key Path</Label>
              <Input
                placeholder="/etc/ssl/key.pem"
                value={form.keyFile}
                onChange={(e) => onChange({ keyFile: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>uTLS Fingerprint</Label>
              <Select
                value={form.fingerprint || 'chrome'}
                onValueChange={(v) => onChange({ fingerprint: v })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="chrome">chrome</SelectItem>
                  <SelectItem value="firefox">firefox</SelectItem>
                  <SelectItem value="safari">safari</SelectItem>
                  <SelectItem value="ios">ios</SelectItem>
                  <SelectItem value="android">android</SelectItem>
                  <SelectItem value="edge">edge</SelectItem>
                  <SelectItem value="random">random</SelectItem>
                  <SelectItem value="randomized">randomized</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>Allow Insecure</Label>
              <Switch
                checked={form.allowInsecure}
                onCheckedChange={(v) => onChange({ allowInsecure: v })}
              />
            </div>
          </div>

          {/* Pinning & TLS v26 Options */}
          <div className="grid gap-3 sm:grid-cols-2 border-t pt-3">
            <div className="space-y-1.5">
              <Label>Pinned Peer Cert SHA-256</Label>
              <Input
                placeholder="SHA-256 hex/base64 pin(s)"
                value={form.pinnedPeerCertSha256}
                onChange={(e) => onChange({ pinnedPeerCertSha256: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Verify Peer Cert By Name</Label>
              <Input
                placeholder="host1.example.com"
                value={form.verifyPeerCertByName}
                onChange={(e) => onChange({ verifyPeerCertByName: e.target.value })}
              />
            </div>
            {isXray && (
              <>
                <div className="space-y-1.5">
                  <Label>Cipher Suites (optional)</Label>
                  <Input
                    placeholder="TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256"
                    value={form.tlsCipherSuites}
                    onChange={(e) => onChange({ tlsCipherSuites: e.target.value })}
                    className="font-mono text-xs"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Curve Preferences</Label>
                  <Input
                    placeholder="X25519,P-256"
                    value={form.tlsCurvePreferences}
                    onChange={(e) => onChange({ tlsCurvePreferences: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>OCSP Stapling Interval (s)</Label>
                  <Input
                    type="number"
                    min={0}
                    placeholder="3600"
                    value={form.tlsOcspStapling}
                    onChange={(e) => onChange({ tlsOcspStapling: e.target.value })}
                  />
                </div>
                <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                  <Label>Reject Unknown SNI</Label>
                  <Switch
                    checked={form.tlsRejectUnknownSni}
                    onCheckedChange={(v) => onChange({ tlsRejectUnknownSni: v })}
                  />
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Reality */}
      {isXray && form.security === 'reality' && (
        <div className="space-y-3 rounded-lg border p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-xs font-semibold uppercase tracking-wide text-primary">
              Reality Configuration (X25519 / PQ)
            </span>
            <div className="flex flex-wrap items-center gap-1.5">
              <Button
                type="button"
                variant="outline"
                size="sm"
                loading={genBusy === 'mldsa65'}
                onClick={handleGenMldsa65}
              >
                <Sparkles className="h-3.5 w-3.5" /> Generate ML-DSA-65 PQ
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                loading={genBusy === 'reality'}
                onClick={handleGenReality}
              >
                <KeyRound className="h-3.5 w-3.5" /> Generate X25519 Keypair
              </Button>
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Target / Dest</Label>
              <Input
                placeholder="www.google.com:443"
                value={form.realityDest}
                onChange={(e) => onChange({ realityDest: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Server Names (comma-separated)</Label>
              <Input
                placeholder="www.google.com"
                value={form.realityServerNames}
                onChange={(e) => onChange({ realityServerNames: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Private Key (X25519 Base64url)</Label>
              <Input
                value={form.realityPrivateKey}
                onChange={(e) => onChange({ realityPrivateKey: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label>Public Key (X25519 Base64url)</Label>
              <Input
                value={form.realityPublicKey}
                onChange={(e) => onChange({ realityPublicKey: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <Label>Short IDs (comma-separated hex)</Label>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => onChange({ realityShortIds: generateShortId(8) })}
                >
                  <RefreshCw className="h-3 w-3" /> New
                </Button>
              </div>
              <Input
                value={form.realityShortIds}
                onChange={(e) => onChange({ realityShortIds: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label>SpiderX</Label>
              <Input
                value={form.realitySpiderX}
                onChange={(e) => onChange({ realitySpiderX: e.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <Label>ML-DSA-65 Seed (PQ)</Label>
              <Input
                placeholder="Optional mldsa65Seed"
                value={form.mldsa65Seed}
                onChange={(e) => onChange({ mldsa65Seed: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label>ML-DSA-65 Verify (PQ)</Label>
              <Input
                placeholder="Optional mldsa65Verify"
                value={form.mldsa65Verify}
                onChange={(e) => onChange({ mldsa65Verify: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
          </div>

          <details className="rounded-md border bg-muted/20 p-2.5 text-xs">
            <summary className="cursor-pointer font-medium text-muted-foreground select-none">
              Advanced Reality v26 (xver, Time Diff, Client Ver, Limit Fallback)
            </summary>
            <div className="mt-3 grid gap-3 sm:grid-cols-3">
              <div className="space-y-1.5">
                <Label>PROXY Protocol xver</Label>
                <Input
                  type="number"
                  min={0}
                  max={2}
                  value={form.realityXver}
                  onChange={(e) => onChange({ realityXver: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <Label>Max Time Diff (ms)</Label>
                <Input
                  type="number"
                  min={0}
                  placeholder="0"
                  value={form.realityMaxTimeDiff}
                  onChange={(e) => onChange({ realityMaxTimeDiff: e.target.value })}
                />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>Debug show</Label>
                <Switch
                  checked={form.realityShow}
                  onCheckedChange={(v) => onChange({ realityShow: v })}
                />
              </div>
              <div className="space-y-1.5">
                <Label>Min Client Ver</Label>
                <Input
                  placeholder="e.g. 25.1.1"
                  value={form.realityMinClientVer}
                  onChange={(e) => onChange({ realityMinClientVer: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <Label>Max Client Ver</Label>
                <Input
                  placeholder="optional"
                  value={form.realityMaxClientVer}
                  onChange={(e) => onChange({ realityMaxClientVer: e.target.value })}
                />
              </div>
              <div className="sm:col-span-3 grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label>limitFallbackUpload (JSON)</Label>
                  <Textarea
                    rows={2}
                    placeholder='{"afterBytes": 0, "bytesPerSec": 0}'
                    value={form.realityLimitFallbackUploadJson}
                    onChange={(e) => onChange({ realityLimitFallbackUploadJson: e.target.value })}
                    className="font-mono text-xs"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>limitFallbackDownload (JSON)</Label>
                  <Textarea
                    rows={2}
                    placeholder='{"afterBytes": 0, "bytesPerSec": 0}'
                    value={form.realityLimitFallbackDownloadJson}
                    onChange={(e) => onChange({ realityLimitFallbackDownloadJson: e.target.value })}
                    className="font-mono text-xs"
                  />
                </div>
              </div>
            </div>
          </details>
        </div>
      )}

      {/* Hysteria2 Obfuscation, BBR & Chrome Parrot */}
      {form.protocol === 'HYSTERIA2' && (
        <div className="space-y-3 rounded-lg border p-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>Obfuscation</Label>
              <Select
                value={form.hy2ObfsType}
                onValueChange={(v) => onChange({ hy2ObfsType: v as Hy2ObfsType })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">None</SelectItem>
                  <SelectItem value="salamander">Salamander</SelectItem>
                  <SelectItem value="gecko">Gecko</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {form.hy2ObfsType !== 'none' && (
              <div className="space-y-1.5">
                <Label>Obfs Password</Label>
                <Input
                  value={form.hy2ObfsPassword}
                  onChange={(e) => onChange({ hy2ObfsPassword: e.target.value })}
                />
              </div>
            )}
            {form.hy2ObfsType === 'gecko' && (
              <>
                <div className="space-y-1.5">
                  <Label>Gecko Min Packet Size</Label>
                  <Input
                    type="number"
                    min={0}
                    value={form.hy2GeckoMinPacketSize}
                    onChange={(e) => onChange({ hy2GeckoMinPacketSize: e.target.value })}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Gecko Max Packet Size</Label>
                  <Input
                    type="number"
                    min={0}
                    value={form.hy2GeckoMaxPacketSize}
                    onChange={(e) => onChange({ hy2GeckoMaxPacketSize: e.target.value })}
                  />
                </div>
              </>
            )}
            <div className="space-y-1.5">
              <Label>BBR Profile</Label>
              <Select
                value={form.hy2BbrProfile}
                onValueChange={(v) => onChange({ hy2BbrProfile: v as Hy2BbrProfile })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">Default</SelectItem>
                  <SelectItem value="conservative">Conservative</SelectItem>
                  <SelectItem value="standard">Standard</SelectItem>
                  <SelectItem value="aggressive">Aggressive</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between rounded-lg border px-3 py-2">
              <Label>Disable Chrome Parrot</Label>
              <Switch
                checked={form.disableChromeParrot}
                onCheckedChange={(v) => onChange({ disableChromeParrot: v })}
              />
            </div>
          </div>
        </div>
      )}

      {/* ECH & FinalMask */}
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-2 rounded-lg border p-3">
          <div className="flex items-center justify-between">
            <Label>ECH (Encrypted Client Hello)</Label>
            <Switch
              checked={form.echEnabled}
              onCheckedChange={(v) => onChange({ echEnabled: v })}
            />
          </div>
          {form.echEnabled && (
            <div className="space-y-2 pt-1">
              <Input
                placeholder="echConfig (Base64)"
                value={form.echConfig}
                onChange={(e) => onChange({ echConfig: e.target.value })}
              />
              <Input
                placeholder="echKeyPath (/etc/ssl/ech.key)"
                value={form.echKeyPath}
                onChange={(e) => onChange({ echKeyPath: e.target.value })}
              />
            </div>
          )}
        </div>

        <div className="space-y-2 rounded-lg border p-3">
          <div className="flex items-center justify-between">
            <Label>FinalMask (Xray v26+)</Label>
            <Switch
              checked={form.finalMaskEnabled}
              onCheckedChange={(v) => onChange({ finalMaskEnabled: v })}
            />
          </div>
          {form.finalMaskEnabled && (
            <div className="space-y-2 pt-1">
              <div className="flex flex-wrap gap-1.5">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-6 px-2 text-[11px]"
                  onClick={() =>
                    onChange({
                      finalMaskJson: JSON.stringify(
                        { type: 'tcp', fragment: { packets: 'tlshello', length: '10-20', interval: '10-20' } },
                        null,
                        2,
                      ),
                    })
                  }
                >
                  TCP Fragment
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-6 px-2 text-[11px]"
                  onClick={() =>
                    onChange({
                      finalMaskJson: JSON.stringify(
                        { type: 'sudoku', password: 'change-me', ascii: 'prefer_ascii' },
                        null,
                        2,
                      ),
                    })
                  }
                >
                  Sudoku
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-6 px-2 text-[11px]"
                  onClick={() =>
                    onChange({
                      finalMaskJson: JSON.stringify(
                        { type: 'header-custom', client: [], server: [] },
                        null,
                        2,
                      ),
                    })
                  }
                >
                  Header Custom
                </Button>
              </div>
              <Textarea
                rows={3}
                placeholder='{"type": "tcp", "fragment": {"packets": "tlshello"}}'
                value={form.finalMaskJson}
                onChange={(e) => onChange({ finalMaskJson: e.target.value })}
                className="font-mono text-xs"
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
