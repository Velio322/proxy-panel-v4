import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { QRCodeSVG } from 'qrcode.react';
import { Check, Copy, Download, QrCode } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { buildSubUrl } from '@/pages/clients/utils';
import { clientsApi, getApiErrorMessage } from '@/lib/api';
import { PROTOCOL_COLORS, cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';
import type { Client, ClientLinkEntry } from '@/types';
import { useCopyFeedback } from '@/hooks/use-copy-feedback';

interface SubscriptionModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  client: Client | null;
}

const SUB_FORMATS = [
  { id: 'default', labelEn: 'Auto / Landing', labelRu: 'Авто / Страница', flag: undefined },
  { id: 'base64', labelEn: 'Base64', labelRu: 'Base64', flag: 'base64' },
  { id: 'clash', labelEn: 'Clash / Mihomo', labelRu: 'Clash / Mihomo', flag: 'clash' },
  { id: 'singbox', labelEn: 'Sing-box', labelRu: 'Sing-box', flag: 'singbox' },
  { id: 'original', labelEn: 'Original JSON', labelRu: 'Xray JSON', flag: 'original' },
  { id: 'json', labelEn: 'JSON (All / Native)', labelRu: 'JSON (Все / Native)', flag: 'json' },
  { id: 'happ', labelEn: 'Happ (crypt4/5)', labelRu: 'Happ (crypt4/5)', flag: 'happ' },
];

const OPENWRT_FORMATS: Array<{ id: 'uci' | 'passwall' | 'openclash' | 'singbox' | 'ssr'; label: string }> = [
  { id: 'uci', label: 'OpenWrt UCI' },
  { id: 'passwall', label: 'Passwall' },
  { id: 'openclash', label: 'OpenClash' },
  { id: 'singbox', label: 'Sing-box' },
  { id: 'ssr', label: 'SSR' },
];

export function SubscriptionModal({ open, onOpenChange, client }: SubscriptionModalProps) {
  const isRu = useUiStore((s) => s.language) === 'ru';
  const [selectedFlag, setSelectedFlag] = useState<string | undefined>(undefined);
  const [copiedKey, copyKey, resetCopiedKey] = useCopyFeedback();
  const [exportText, setExportText] = useState<string>('');
  const [loadingExport, setLoadingExport] = useState(false);
  const [selectedEntryIdx, setSelectedEntryIdx] = useState(0);
  const [qrMode, setQrMode] = useState<'link' | 'config'>('link');

  useEffect(() => {
    setSelectedEntryIdx(0);
    setQrMode('link');
    resetCopiedKey();
    setExportText('');
  }, [open, client?.id, resetCopiedKey]);

  const {
    data: linksData,
    isLoading: linksLoading,
    isError: linksError,
    error: linksErrorObj,
  } = useQuery({
    queryKey: ['client-links', client?.id],
    queryFn: () => clientsApi.getLinks(client!.id),
    enabled: open && Boolean(client?.id),
  });

  if (!client) return null;

  const entries: ClientLinkEntry[] = linksData?.entries ?? [];
  const safeIdx = entries.length > 0 ? Math.min(selectedEntryIdx, entries.length - 1) : 0;
  const activeEntry: ClientLinkEntry | null = entries[safeIdx] ?? null;

  const formattedConfig = activeEntry?.config
    ? JSON.stringify(activeEntry.config, null, 2)
    : null;
  const effectiveQrMode = qrMode === 'config' && activeEntry?.config ? 'config' : 'link';
  const entryQrValue = activeEntry
    ? effectiveQrMode === 'config' && activeEntry.config
      ? JSON.stringify(activeEntry.config)
      : activeEntry.link
    : '';

  const subUrl = buildSubUrl(client.subToken, selectedFlag);

  const copyText = async (key: string, text: string) => {
    try {
      await copyKey(text, key);
    } catch {
      toast({ title: isRu ? 'Не удалось скопировать' : 'Copy failed', variant: 'destructive' });
    }
  };

  const downloadJsonConfig = (entry: ClientLinkEntry) => {
    if (!entry.config) return;
    const content = JSON.stringify(entry.config, null, 2);
    const safeTag = entry.tag.replace(/[^a-zA-Z0-9._-]+/g, '-');
    const blob = new Blob([content], { type: 'application/json;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${client.username}-${safeTag}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const loadOpenWrt = async (format: 'uci' | 'passwall' | 'openclash' | 'singbox' | 'ssr') => {
    setLoadingExport(true);
    try {
      const content = await clientsApi.exportConfig(client.id, format);
      setExportText(content);
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка экспорта' : 'Export failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setLoadingExport(false);
    }
  };

  const downloadExport = () => {
    if (!exportText) return;
    const blob = new Blob([exportText], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${client.username}-openwrt.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <QrCode className="h-4 w-4 text-primary" />
            {isRu ? `Подписка и ключи: ${client.username}` : `Subscription: ${client.username}`}
          </DialogTitle>
          <DialogDescription>
            {isRu
              ? 'QR-коды и ссылки для каждого протокола, JSON-конфиги и общая ссылка подписки.'
              : 'View per-protocol QR codes and links, copy native configs, scan the subscription QR, or export for OpenWrt.'}
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="nodes">
          <TabsList>
            <TabsTrigger value="nodes">
              {isRu ? `Ключи и QR (${entries.length})` : `Nodes & QR (${entries.length})`}
            </TabsTrigger>
            <TabsTrigger value="links">{isRu ? 'Ссылка подписки' : 'Subscription URL'}</TabsTrigger>
            <TabsTrigger value="openwrt">{isRu ? 'Экспорт OpenWrt' : 'OpenWrt Export'}</TabsTrigger>
          </TabsList>

          <TabsContent value="nodes" className="space-y-4">
            {linksLoading ? (
              <div className="space-y-2 py-2">
                <Skeleton className="h-9 w-full" />
                <Skeleton className="h-52 w-full" />
              </div>
            ) : linksError ? (
              <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-400">
                {isRu ? 'Не удалось загрузить ссылки: ' : 'Failed to load client links: '}
                {getApiErrorMessage(linksErrorObj)}
              </div>
            ) : entries.length === 0 ? (
              <div className="rounded-lg border border-dashed p-5 text-center text-xs text-muted-foreground">
                {isRu
                  ? 'Нет активных ссылок для этого клиента. Проверьте выбранные протоколы, привязку к нодам и что нода онлайн.'
                  : 'No active links generated for this client. Verify that client protocols, inbound bindings, and node online status are enabled.'}
              </div>
            ) : (
              <>
                <div className="flex flex-wrap gap-1.5">
                  {entries.map((entry, idx) => {
                    const isSelected = idx === safeIdx;
                    return (
                      <Button
                        key={`${entry.protocol}-${entry.tag}-${idx}`}
                        type="button"
                        size="sm"
                        variant={isSelected ? 'default' : 'outline'}
                        onClick={() => {
                          setSelectedEntryIdx(idx);
                          setQrMode('link');
                        }}
                        className="gap-1.5"
                      >
                        <Badge className={cn('border px-1.5 py-0 text-[10px]', PROTOCOL_COLORS[entry.protocol])}>
                          {entry.protocol}
                        </Badge>
                        <span>
                          {entry.tag} :{entry.port}
                        </span>
                      </Button>
                    );
                  })}
                </div>

                {activeEntry && (
                  <div className="space-y-3 rounded-xl border bg-background p-4">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex items-center gap-2 text-xs">
                        <Badge className={cn('border text-[10px]', PROTOCOL_COLORS[activeEntry.protocol])}>
                          {activeEntry.protocol}
                        </Badge>
                        <span className="font-semibold">{activeEntry.tag}</span>
                        <span className="font-mono text-muted-foreground">
                          {activeEntry.host}:{activeEntry.port}
                        </span>
                      </div>
                      {activeEntry.config && (
                        <div className="flex items-center gap-1">
                          <Button
                            type="button"
                            size="sm"
                            variant={effectiveQrMode === 'link' ? 'default' : 'outline'}
                            className="h-7 px-2 text-xs"
                            onClick={() => setQrMode('link')}
                          >
                            {isRu ? 'Ссылка' : 'Share Link'}
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            variant={effectiveQrMode === 'config' ? 'default' : 'outline'}
                            className="h-7 px-2 text-xs"
                            onClick={() => setQrMode('config')}
                          >
                            {isRu ? 'JSON конфиг' : 'JSON Config'}
                          </Button>
                        </div>
                      )}
                    </div>

                    <div className="flex flex-col items-center gap-3">
                      <div className="rounded-lg bg-white p-3">
                        <QRCodeSVG value={entryQrValue} size={184} />
                      </div>
                    </div>

                    <div className="space-y-1.5">
                      <div className="text-[11px] text-muted-foreground">
                        {isRu ? 'Ссылка для подключения' : 'Share Link'}
                      </div>
                      <div className="flex w-full gap-2">
                        <Input readOnly value={activeEntry.link} className="font-mono text-xs" />
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() => copyText('entry-link', activeEntry.link)}
                        >
                          {copiedKey === 'entry-link' ? (
                            <Check className="h-4 w-4 text-emerald-400" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                    </div>

                    {activeEntry.protocol === 'NAIVEPROXY' && (
                      <div className="rounded-md bg-muted/40 px-3 py-2 text-[11px] text-muted-foreground">
                        {isRu ? (
                          <>
                            Ссылку <span className="font-mono text-foreground">naive+https://</span> открывайте в Exclave, NekoBox, Husi или Karing; для официального клиента <span className="font-mono text-foreground">naive</span> используйте JSON конфиг ниже.
                          </>
                        ) : (
                          <>
                            Use <span className="font-mono text-foreground">naive+https://</span> in Exclave, NekoBox, Husi, or Karing; use the native JSON config below for the official <span className="font-mono text-foreground">naive</span> client.
                          </>
                        )}
                      </div>
                    )}
                    {activeEntry.protocol === 'MIERU' && (
                      <div className="rounded-md bg-muted/40 px-3 py-2 text-[11px] text-muted-foreground">
                        {isRu ? (
                          <>
                            Ссылку <span className="font-mono text-foreground">mierus://</span> используйте в Exclave, NekoBox, Karing или <span className="font-mono text-foreground">mieru import config</span>; JSON-профиль ниже подходит для <span className="font-mono text-foreground">mieru apply config</span>.
                          </>
                        ) : (
                          <>
                            Use <span className="font-mono text-foreground">mierus://</span> with <span className="font-mono text-foreground">mieru import config</span>, Exclave, NekoBox, or Karing; use the JSON profile below for <span className="font-mono text-foreground">mieru apply config</span>.
                          </>
                        )}
                      </div>
                    )}

                    {formattedConfig && (
                      <div className="space-y-1.5">
                        <div className="flex items-center justify-between">
                          <span className="text-[11px] text-muted-foreground">
                            {isRu ? 'Нативный JSON конфиг' : 'Native JSON Config'}
                          </span>
                          <div className="flex items-center gap-1.5">
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              className="h-7 px-2 text-xs"
                              onClick={() => copyText('entry-json', formattedConfig)}
                            >
                              {copiedKey === 'entry-json' ? (
                                <Check className="h-3.5 w-3.5 text-emerald-400" />
                              ) : (
                                <Copy className="h-3.5 w-3.5" />
                              )}
                              {isRu ? 'Копировать JSON' : 'Copy JSON'}
                            </Button>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              className="h-7 px-2 text-xs"
                              onClick={() => downloadJsonConfig(activeEntry)}
                            >
                              <Download className="h-3.5 w-3.5" />
                              {isRu ? 'Скачать JSON' : 'Download JSON'}
                            </Button>
                          </div>
                        </div>
                        <pre className="max-h-48 overflow-auto rounded-lg border bg-muted/30 p-3 font-mono text-xs">
                          {formattedConfig}
                        </pre>
                      </div>
                    )}

                    <div className="flex justify-end border-t pt-2">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => copyText('all-links', entries.map((e) => e.link).join('\n'))}
                      >
                        {copiedKey === 'all-links' ? (
                          <Check className="h-3.5 w-3.5 text-emerald-400" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                        {isRu ? `Скопировать все ссылки (${entries.length})` : `Copy All Links (${entries.length})`}
                      </Button>
                    </div>
                  </div>
                )}
              </>
            )}
          </TabsContent>

          <TabsContent value="links" className="space-y-4">
            <div className="flex flex-wrap gap-1.5">
              {SUB_FORMATS.map((fmt) => (
                <Button
                  key={fmt.id}
                  type="button"
                  size="sm"
                  variant={selectedFlag === fmt.flag ? 'default' : 'outline'}
                  onClick={() => setSelectedFlag(fmt.flag)}
                >
                  {isRu ? fmt.labelRu : fmt.labelEn}
                </Button>
              ))}
            </div>

            <div className="flex flex-col items-center gap-3 rounded-xl border bg-background p-4">
              <div className="rounded-lg bg-white p-3">
                <QRCodeSVG value={subUrl} size={180} />
              </div>
              <div className="flex w-full gap-2">
                <Input readOnly value={subUrl} className="font-mono text-xs" />
                <Button type="button" variant="outline" onClick={() => copyText('sub-url', subUrl)}>
                  {copiedKey === 'sub-url' ? (
                    <Check className="h-4 w-4 text-emerald-400" />
                  ) : (
                    <Copy className="h-4 w-4" />
                  )}
                </Button>
              </div>
              <div className="flex flex-wrap justify-center gap-2 text-xs">
                <a
                  href={`happ://add/${encodeURIComponent(subUrl)}`}
                  className="rounded-md border px-2.5 py-1 hover:bg-accent"
                >
                  {isRu ? 'Открыть в Happ' : 'Open in Happ'}
                </a>
                <a
                  href={`karing://install-config?url=${encodeURIComponent(subUrl)}&name=${encodeURIComponent(client.username)}`}
                  className="rounded-md border px-2.5 py-1 hover:bg-accent"
                >
                  {isRu ? 'Открыть в Karing' : 'Open in Karing'}
                </a>
                <a
                  href={`sn://subscription?url=${encodeURIComponent(subUrl)}&name=${encodeURIComponent(client.username)}`}
                  className="rounded-md border px-2.5 py-1 hover:bg-accent"
                >
                  {isRu ? 'Открыть в NekoBox' : 'Open in NekoBox'}
                </a>
                <a
                  href={`throne://addsub/?url=${encodeURIComponent(subUrl)}&name=${encodeURIComponent(client.username)}`}
                  className="rounded-md border px-2.5 py-1 hover:bg-accent"
                >
                  {isRu ? 'Открыть в Throne (ПК)' : 'Open in Throne (PC)'}
                </a>
                <a
                  href={subUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="rounded-md border border-primary/40 bg-primary/10 px-2.5 py-1 text-primary hover:bg-primary/20"
                >
                  {isRu ? 'Страница подписки' : 'Open Sub Page'}
                </a>
              </div>
            </div>
          </TabsContent>

          <TabsContent value="openwrt" className="space-y-3">
            <div className="flex flex-wrap gap-2">
              {OPENWRT_FORMATS.map((fmt) => (
                <Button
                  key={fmt.id}
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={loadingExport}
                  onClick={() => loadOpenWrt(fmt.id)}
                >
                  {fmt.label}
                </Button>
              ))}
            </div>
            {exportText && (
              <div className="space-y-2">
                <pre className="max-h-56 overflow-auto rounded-lg border bg-background p-3 font-mono text-xs">
                  {exportText}
                </pre>
                <div className="flex justify-end gap-2">
                  <Button type="button" size="sm" variant="outline" onClick={() => copyText('openwrt', exportText)}>
                    {copiedKey === 'openwrt' ? (
                      <Check className="h-3.5 w-3.5 text-emerald-400" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    {isRu ? 'Копировать' : 'Copy'}
                  </Button>
                  <Button type="button" size="sm" onClick={downloadExport}>
                    <Download className="h-3.5 w-3.5" /> {isRu ? 'Скачать' : 'Download'}
                  </Button>
                </div>
              </div>
            )}
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
