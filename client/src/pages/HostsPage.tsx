import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Edit, Globe, Network, Plus, Power, Shield, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { Checkbox } from '@/components/ui/checkbox';
import { PageHero, RackEmptyState, SplitBar } from '@/components/ui/PagePrimitives';
import { getApiErrorMessage, hostsApi, inboundsApi } from '@/lib/api';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { HostOverride } from '@/types';

export function HostsPage() {
  const qc = useQueryClient();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [inboundFilter, setInboundFilter] = useState('all');
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<HostOverride | null>(null);

  const [inboundId, setInboundId] = useState('');
  const [remark, setRemark] = useState('');
  const [address, setAddress] = useState('');
  const [port, setPort] = useState('');
  const [sni, setSni] = useState('');
  const [host, setHost] = useState('');
  const [path, setPath] = useState('');
  const [alpn, setAlpn] = useState('');
  const [fingerprint, setFingerprint] = useState('chrome');
  const [vlessRouteId, setVlessRouteId] = useState('');
  const [xhttpExtraJson, setXhttpExtraJson] = useState('');
  const [finalMaskJson, setFinalMaskJson] = useState('');
  const [fragmentPackets, setFragmentPackets] = useState('');
  const [fragmentLength, setFragmentLength] = useState('');
  const [fragmentInterval, setFragmentInterval] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [allowInsecureOverride, setAllowInsecureOverride] = useState<'inherit' | 'true' | 'false'>('inherit');
  const [pinnedPeerCertSha256, setPinnedPeerCertSha256] = useState('');
  const [verifyPeerCertByName, setVerifyPeerCertByName] = useState('');
  const [mihomoIpVersion, setMihomoIpVersion] = useState('default');
  const [serverDescription, setServerDescription] = useState('');
  const [selectedHostIds, setSelectedHostIds] = useState<string[]>([]);
  const [isCdn, setIsCdn] = useState(false);
  const [shuffleHost, setShuffleHost] = useState(false);
  const [enable, setEnable] = useState(true);

  const { data: inbounds = [] } = useQuery({
    queryKey: ['inbounds'],
    queryFn: () => inboundsApi.list(),
  });

  const { data: hosts = [], isLoading } = useQuery({
    queryKey: ['hosts', inboundFilter],
    queryFn: () => hostsApi.list({ inboundId: inboundFilter === 'all' ? undefined : inboundFilter }),
  });

  const stats = useMemo(() => {
    let enabled = 0;
    let cdn = 0;
    let fragmentCount = 0;
    const inboundSet = new Set<string>();
    for (const h of hosts) {
      if (h.enable) enabled += 1;
      if (h.isCdn) cdn += 1;
      if (h.fragment) fragmentCount += 1;
      inboundSet.add(h.inboundId);
    }
    return {
      total: hosts.length,
      enabled,
      disabled: hosts.length - enabled,
      cdn,
      fragmentCount,
      inboundsCovered: inboundSet.size,
    };
  }, [hosts]);

  const openCreate = () => {
    setEditing(null);
    setInboundId(inbounds[0]?.id ?? '');
    setRemark('');
    setAddress('');
    setPort('');
    setSni('');
    setHost('');
    setPath('');
    setAlpn('');
    setFingerprint('chrome');
    setVlessRouteId('');
    setXhttpExtraJson('');
    setFinalMaskJson('');
    setFragmentPackets('');
    setFragmentLength('');
    setFragmentInterval('');
    setTagsText('');
    setAllowInsecureOverride('inherit');
    setPinnedPeerCertSha256('');
    setVerifyPeerCertByName('');
    setMihomoIpVersion('default');
    setServerDescription('');
    setIsCdn(false);
    setShuffleHost(false);
    setEnable(true);
    setModalOpen(true);
  };

  const openEdit = (h: HostOverride) => {
    setEditing(h);
    setInboundId(h.inboundId);
    setRemark(h.remark);
    setAddress(h.address);
    setPort(h.port ? String(h.port) : '');
    setSni(h.sni ?? '');
    setHost(h.host ?? '');
    setPath(h.path ?? '');
    setAlpn(h.alpn ?? '');
    setFingerprint(h.fingerprint ?? 'chrome');
    setVlessRouteId(h.vlessRouteId ?? '');
    setXhttpExtraJson(h.xhttpExtra ? JSON.stringify(h.xhttpExtra, null, 2) : '');
    setFinalMaskJson(h.finalMask ? JSON.stringify(h.finalMask, null, 2) : '');
    setFragmentPackets(h.fragment?.packets ?? '');
    setFragmentLength(h.fragment?.length ?? '');
    setFragmentInterval(h.fragment?.interval ?? '');
    setTagsText(Array.isArray(h.tags) ? h.tags.join(', ') : '');
    setAllowInsecureOverride(
      h.allowInsecure === true ? 'true' : h.allowInsecure === false ? 'false' : 'inherit',
    );
    setPinnedPeerCertSha256(h.pinnedPeerCertSha256 ?? '');
    setVerifyPeerCertByName(h.verifyPeerCertByName ?? '');
    setMihomoIpVersion(h.mihomoIpVersion || 'default');
    setServerDescription(h.serverDescription ?? '');
    setIsCdn(Boolean(h.isCdn));
    setShuffleHost(Boolean(h.shuffleHost));
    setEnable(h.enable);
    setModalOpen(true);
  };

  const saveMut = useMutation({
    mutationFn: async () => {
      let xhttpExtra: Record<string, unknown> | null = null;
      if (xhttpExtraJson.trim()) xhttpExtra = JSON.parse(xhttpExtraJson);
      let finalMask: Record<string, unknown> | null = null;
      if (finalMaskJson.trim()) finalMask = JSON.parse(finalMaskJson);

      const fragment =
        fragmentPackets.trim() || fragmentLength.trim() || fragmentInterval.trim()
          ? {
              packets: fragmentPackets.trim() || undefined,
              length: fragmentLength.trim() || undefined,
              interval: fragmentInterval.trim() || undefined,
            }
          : null;

      const tags = tagsText
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean);

      const payload: Partial<HostOverride> = {
        inboundId,
        remark: remark.trim(),
        address: address.trim(),
        port: port.trim() ? Number(port) : null,
        sni: sni.trim() || null,
        host: host.trim() || null,
        path: path.trim() || null,
        alpn: alpn.trim() || null,
        fingerprint: fingerprint !== 'none' ? fingerprint : null,
        vlessRouteId: vlessRouteId.trim() || null,
        xhttpExtra,
        finalMask,
        fragment,
        tags,
        allowInsecure:
          allowInsecureOverride === 'inherit'
            ? null
            : allowInsecureOverride === 'true',
        pinnedPeerCertSha256: pinnedPeerCertSha256.trim() || null,
        verifyPeerCertByName: verifyPeerCertByName.trim() || null,
        mihomoIpVersion: mihomoIpVersion !== 'default' ? mihomoIpVersion : null,
        serverDescription: serverDescription.trim() || null,
        isCdn,
        shuffleHost,
        enable,
      };
      if (editing) return hostsApi.update(editing.id, payload);
      return hostsApi.create(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['hosts'] });
      setModalOpen(false);
      toast({
        title: editing
          ? isRu
            ? 'Хост обновлён'
            : 'Host updated'
          : isRu
            ? 'Хост создан'
            : 'Host created',
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка сохранения' : 'Save failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const bulkMut = useMutation({
    mutationFn: (action: 'enable' | 'disable' | 'delete') =>
      hostsApi.bulk({ action, hostIds: selectedHostIds }),
    onSuccess: (res, action) => {
      qc.invalidateQueries({ queryKey: ['hosts'] });
      setSelectedHostIds([]);
      toast({
        title: isRu ? `Обновлено хостов (${action}): ${res.affected}` : `Bulk ${action}: ${res.affected} host(s)`,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка массовой операции' : 'Bulk action failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const toggleSelectHost = (id: string) => {
    setSelectedHostIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  };

  const toggleMut = useMutation({
    mutationFn: (id: string) => hostsApi.toggle(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['hosts'] }),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => hostsApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['hosts'] });
      toast({ title: isRu ? 'Хост удалён' : 'Host deleted', variant: 'success' });
    },
  });

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Хосты подписки (Host Overrides)' : 'Subscription Hosts'}
        subtitle={
          isRu
            ? 'Переопределение адресов, CDN-доменов, SNI, путей, фрагментации и FinalMask в клиентских ссылках подписки'
            : 'Per-inbound host overrides (CDN address, SNI, path, fragment, FinalMask) applied to client subscription links.'
        }
        icon={<Globe className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.enabled}/{stats.total} {isRu ? 'активны' : 'active'}
          </Badge>
        }
        actions={
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-4 w-4" />
            <span>{isRu ? 'Добавить хост' : 'Add Host Override'}</span>
          </Button>
        }
      />

      {/* 4-Card KPI Ribbon */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card className="p-4 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Всего хостов' : 'Total Hosts'}</span>
            <Globe className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {stats.enabled} / {stats.total}
          </div>
          <SplitBar
            segments={[
              { pct: stats.total > 0 ? (stats.enabled / stats.total) * 100 : 100, className: 'bg-emerald-400' },
              { pct: stats.total > 0 ? (stats.disabled / stats.total) * 100 : 0, className: 'bg-zinc-600' },
            ]}
          />
        </Card>

        <Card className="p-4 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'CDN / Прокси-хосты' : 'CDN Enabled'}</span>
            <Shield className="h-4 w-4 text-cyan-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.cdn}</div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Маршрутизация через Edge/CDN' : 'Edge CDN domain overrides'}
          </div>
        </Card>

        <Card className="p-4 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Фрагментация TLS' : 'TLS Fragment'}</span>
            <Network className="h-4 w-4 text-primary" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">{stats.fragmentCount}</div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Обход DPI-фильтрации по SNI' : 'DPI packet fragmentation profiles'}
          </div>
        </Card>

        <Card className="p-4 space-y-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{isRu ? 'Покрыто протоколов' : 'Covered Inbounds'}</span>
            <Network className="h-4 w-4 text-amber-400" />
          </div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {stats.inboundsCovered} / {inbounds.length}
          </div>
          <div className="text-[11px] text-muted-foreground">
            {isRu ? 'Инбаундов с переопределением' : 'Inbounds with custom hosts'}
          </div>
        </Card>
      </div>

      <Card>
        <CardHeader className="pb-3 flex flex-row items-center justify-between gap-3">
          <CardTitle className="text-sm">
            {isRu ? `Список хостов (${hosts.length})` : `Host Overrides (${hosts.length})`}
          </CardTitle>
          <div className="w-56">
            <Select value={inboundFilter} onValueChange={setInboundFilter}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{isRu ? 'Все протоколы' : 'All Inbounds'}</SelectItem>
                {inbounds.map((ib) => (
                  <SelectItem key={ib.id} value={ib.id}>
                    {ib.tag} ({ib.protocol})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </CardHeader>
        <CardContent>
          {selectedHostIds.length > 0 && (
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/30 bg-primary/5 px-3 py-2 text-xs">
              <span className="font-medium">
                {isRu
                  ? `Выбрано хостов: ${selectedHostIds.length}`
                  : `${selectedHostIds.length} host${selectedHostIds.length === 1 ? '' : 's'} selected`}
              </span>
              <div className="flex flex-wrap items-center gap-1.5">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={bulkMut.isPending}
                  onClick={() => bulkMut.mutate('enable')}
                >
                  {isRu ? 'Включить' : 'Enable'}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={bulkMut.isPending}
                  onClick={() => bulkMut.mutate('disable')}
                >
                  {isRu ? 'Отключить' : 'Disable'}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="destructive"
                  disabled={bulkMut.isPending}
                  onClick={() => bulkMut.mutate('delete')}
                >
                  {isRu ? 'Удалить' : 'Delete'}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={() => setSelectedHostIds([])}
                >
                  {isRu ? 'Сбросить' : 'Clear'}
                </Button>
              </div>
            </div>
          )}
          {isLoading ? (
            <div className="py-8 text-center text-sm text-muted-foreground">
              {isRu ? 'Загрузка хостов...' : 'Loading hosts...'}
            </div>
          ) : hosts.length === 0 ? (
            <RackEmptyState
              icon={Globe}
              title={isRu ? 'Хосты подписки не настроены' : 'No host overrides configured'}
              description={
                isRu
                  ? 'Добавьте переопределение хоста (CDN, внешний домен, SNI или Fragment) для выдачи в клиентских подписках.'
                  : 'Create a host override to customize CDN domains, SNI, or TLS fragmentation in client subscription links.'
              }
              action={
                <Button size="sm" onClick={openCreate}>
                  <Plus className="h-4 w-4" />
                  <span>{isRu ? 'Добавить хост' : 'Add Host Override'}</span>
                </Button>
              }
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="data-table w-full text-sm">
                <thead className="bg-muted/30">
                  <tr className="border-b text-left text-xs text-muted-foreground">
                    <th className="w-9 py-2.5 px-3">
                      <Checkbox
                        checked={hosts.length > 0 && hosts.every((h) => selectedHostIds.includes(h.id))}
                        onCheckedChange={(checked) =>
                          setSelectedHostIds(checked === true ? hosts.map((h) => h.id) : [])
                        }
                        aria-label="Select all hosts"
                      />
                    </th>
                    <th className="py-2.5 px-3">{isRu ? 'Название (Remark)' : 'Remark'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Протокол' : 'Inbound'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Адрес : Порт' : 'Address : Port'}</th>
                    <th className="py-2.5 px-3">SNI / Host / Path</th>
                    <th className="py-2.5 px-3">{isRu ? 'Теги и опции' : 'Tags & Flags'}</th>
                    <th className="py-2.5 px-3">{isRu ? 'Статус' : 'Status'}</th>
                    <th className="py-2.5 px-3 text-right">{isRu ? 'Действия' : 'Actions'}</th>
                  </tr>
                </thead>
                <tbody>
                  {hosts.map((h) => (
                    <tr key={h.id} className="border-b last:border-b-0 hover:bg-accent/30">
                      <td className="py-2.5 px-3">
                        <Checkbox
                          checked={selectedHostIds.includes(h.id)}
                          onCheckedChange={() => toggleSelectHost(h.id)}
                          aria-label={`Select ${h.remark}`}
                        />
                      </td>
                      <td className="py-2.5 px-3 font-medium">
                        <div className="flex items-center gap-2">
                          <Globe className="h-4 w-4 shrink-0 text-primary" />
                          <div className="min-w-0">
                            <div className="truncate">{h.remark}</div>
                            {h.serverDescription && (
                              <div className="truncate text-[11px] font-normal text-muted-foreground">
                                {h.serverDescription}
                              </div>
                            )}
                          </div>
                        </div>
                      </td>
                      <td className="py-2.5 px-3 text-xs">{h.inbound?.tag ?? h.inboundId}</td>
                      <td className="py-2.5 px-3 font-mono text-xs">
                        {h.address}
                        {h.port ? `:${h.port}` : ''}
                      </td>
                      <td className="py-2.5 px-3 text-xs text-muted-foreground">
                        {[h.sni, h.host, h.path].filter(Boolean).join(' · ') || '—'}
                      </td>
                      <td className="py-2.5 px-3">
                        <div className="flex flex-wrap gap-1">
                          {Array.isArray(h.tags) &&
                            h.tags.map((tag) => (
                              <Badge key={tag} variant="secondary" className="text-[10px]">
                                {tag}
                              </Badge>
                            ))}
                          {h.isCdn && <Badge variant="outline">CDN</Badge>}
                          {h.shuffleHost && <Badge variant="outline">Shuffle</Badge>}
                          {h.fragment && <Badge variant="outline">Fragment</Badge>}
                          {h.allowInsecure === true && <Badge variant="outline">allowInsecure</Badge>}
                          {h.mihomoIpVersion && <Badge variant="outline">{h.mihomoIpVersion}</Badge>}
                          {h.pinnedPeerCertSha256 && <Badge variant="outline">PinCert</Badge>}
                        </div>
                      </td>
                      <td className="py-2.5 px-3">
                        <Badge variant={h.enable ? 'success' : 'secondary'}>
                          {h.enable ? (isRu ? 'Включён' : 'Enabled') : isRu ? 'Отключён' : 'Disabled'}
                        </Badge>
                      </td>
                      <td className="py-2.5 px-3 text-right">
                        <div className="inline-flex gap-1">
                          <Button size="icon" variant="ghost" onClick={() => toggleMut.mutate(h.id)}>
                            <Power className="h-4 w-4" />
                          </Button>
                          <Button size="icon" variant="ghost" onClick={() => openEdit(h)}>
                            <Edit className="h-4 w-4" />
                          </Button>
                          <Button size="icon" variant="ghost" onClick={() => deleteMut.mutate(h.id)}>
                            <Trash2 className="h-4 w-4 text-red-400" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>
              {editing
                ? isRu
                  ? `Редактировать хост: ${editing.remark}`
                  : `Edit Host: ${editing.remark}`
                : isRu
                  ? 'Добавить хост подписки'
                  : 'Add Host Override'}
            </DialogTitle>
            <DialogDescription>
              {isRu
                ? 'Переопределяет поля в выдаваемой клиентам подписке, не меняя слушающий порт на самой ноде.'
                : "Overrides subscription output fields without altering the node's listening port configuration."}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Протокол (Inbound)' : 'Inbound'}</Label>
                <Select value={inboundId || 'none'} onValueChange={(v) => setInboundId(v === 'none' ? '' : v)}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {inbounds.length === 0 && <SelectItem value="none">{isRu ? 'Нет протоколов' : 'No inbounds'}</SelectItem>}
                    {inbounds.map((ib) => (
                      <SelectItem key={ib.id} value={ib.id}>
                        {ib.tag} ({ib.protocol} :{ib.port})
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Название (Remark)' : 'Remark'}</Label>
                <Input placeholder="{tag} - CDN" value={remark} onChange={(e) => setRemark(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Адрес / CDN IP / Домен' : 'Address / CDN IP / Domain'}</Label>
                <Input placeholder="cdn.example.com" value={address} onChange={(e) => setAddress(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Порт (опционально)' : 'Port Override (optional)'}</Label>
                <Input type="number" min={1} max={65535} value={port} onChange={(e) => setPort(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>SNI Override</Label>
                <Input value={sni} onChange={(e) => setSni(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>HTTP / WS Host Override</Label>
                <Input value={host} onChange={(e) => setHost(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Path Override</Label>
                <Input value={path} onChange={(e) => setPath(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>ALPN</Label>
                <Input placeholder="h2,http/1.1" value={alpn} onChange={(e) => setAlpn(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Fingerprint</Label>
                <Select value={fingerprint} onValueChange={setFingerprint}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">Default</SelectItem>
                    <SelectItem value="chrome">chrome</SelectItem>
                    <SelectItem value="firefox">firefox</SelectItem>
                    <SelectItem value="safari">safari</SelectItem>
                    <SelectItem value="ios">ios</SelectItem>
                    <SelectItem value="android">android</SelectItem>
                    <SelectItem value="randomized">randomized</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>VLESS Route ID</Label>
                <Input value={vlessRouteId} onChange={(e) => setVlessRouteId(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Теги хоста (через запятую)' : 'Host Tags (comma-separated)'}</Label>
                <Input placeholder="ru, cdn, bypass" value={tagsText} onChange={(e) => setTagsText(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>{isRu ? 'Описание сервера' : 'Server Description (optional)'}</Label>
                <Input
                  placeholder="Edge CDN node for streaming"
                  value={serverDescription}
                  onChange={(e) => setServerDescription(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>allowInsecure Override</Label>
                <Select
                  value={allowInsecureOverride}
                  onValueChange={(v) => setAllowInsecureOverride(v as 'inherit' | 'true' | 'false')}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="inherit">Inherit from Inbound</SelectItem>
                    <SelectItem value="false">Force Secure (false)</SelectItem>
                    <SelectItem value="true">Allow Insecure (true)</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>Mihomo / Clash IP Version</Label>
                <Select value={mihomoIpVersion} onValueChange={setMihomoIpVersion}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">Default (Mihomo Auto)</SelectItem>
                    <SelectItem value="dual">dual</SelectItem>
                    <SelectItem value="ipv4">ipv4</SelectItem>
                    <SelectItem value="ipv6">ipv6</SelectItem>
                    <SelectItem value="ipv4-prefer">ipv4-prefer</SelectItem>
                    <SelectItem value="ipv6-prefer">ipv6-prefer</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1.5">
                <Label>Pinned Peer Cert SHA-256</Label>
                <Input
                  placeholder="sha256 hex digest"
                  value={pinnedPeerCertSha256}
                  onChange={(e) => setPinnedPeerCertSha256(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label>Verify Peer Cert By Name</Label>
                <Input
                  placeholder="origin.example.com"
                  value={verifyPeerCertByName}
                  onChange={(e) => setVerifyPeerCertByName(e.target.value)}
                />
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-3">
              <div className="space-y-1.5">
                <Label>Fragment Packets</Label>
                <Input placeholder="tlshello" value={fragmentPackets} onChange={(e) => setFragmentPackets(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Fragment Length</Label>
                <Input placeholder="10-20" value={fragmentLength} onChange={(e) => setFragmentLength(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>Fragment Interval</Label>
                <Input placeholder="10-20" value={fragmentInterval} onChange={(e) => setFragmentInterval(e.target.value)} />
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>xhttpExtra JSON (optional)</Label>
                <Textarea
                  rows={2}
                  value={xhttpExtraJson}
                  onChange={(e) => setXhttpExtraJson(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
              <div className="space-y-1.5">
                <Label>finalMask JSON (optional)</Label>
                <Textarea
                  rows={2}
                  value={finalMaskJson}
                  onChange={(e) => setFinalMaskJson(e.target.value)}
                  className="font-mono text-xs"
                />
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-3">
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>CDN Mode</Label>
                <Switch checked={isCdn} onCheckedChange={setIsCdn} />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>Shuffle Host</Label>
                <Switch checked={shuffleHost} onCheckedChange={setShuffleHost} />
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2">
                <Label>{isRu ? 'Включён' : 'Enabled'}</Label>
                <Switch checked={enable} onCheckedChange={setEnable} />
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setModalOpen(false)}>
              {isRu ? 'Отмена' : 'Cancel'}
            </Button>
            <Button
              type="button"
              onClick={() => saveMut.mutate()}
              loading={saveMut.isPending}
              disabled={!inboundId || !remark.trim() || !address.trim()}
            >
              {isRu ? 'Сохранить хост' : 'Save Host'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
