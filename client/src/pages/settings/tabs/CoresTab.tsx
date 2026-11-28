import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Download, ExternalLink, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { TabsContent } from '@/components/ui/tabs';
import { coresApi, getApiErrorMessage, nodesApi } from '@/lib/api';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

export function SettingsCoresTab() {
  const qc = useQueryClient();
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const role = useAuthStore((s) => s.user?.role);
  const isAdmin = role === 'SUPER_ADMIN' || role === 'ADMIN';

  const [updatingCoreKey, setUpdatingCoreKey] = useState<string | null>(null);
  const [refreshingCores, setRefreshingCores] = useState(false);

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    enabled: isAdmin,
  });

  const { data: coreCatalog = [] } = useQuery({
    queryKey: ['cores-versions'],
    queryFn: () => coresApi.versions(),
    enabled: isAdmin,
  });

  const handleRefreshCores = async () => {
    setRefreshingCores(true);
    try {
      const fresh = await coresApi.versions(true);
      qc.setQueryData(['cores-versions'], fresh);
      qc.invalidateQueries({ queryKey: ['nodes'] });
      toast({
        title: isRu ? 'Список релизов обновлён' : 'Core catalog refreshed',
        variant: 'success',
      });
    } catch (err) {
      toast({
        title: isRu ? 'Ошибка проверки обновлений' : 'Failed to check updates',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      });
    } finally {
      setRefreshingCores(false);
    }
  };

  const updateCoreMut = useMutation({
    mutationFn: ({ nodeId, core }: { nodeId: string; core: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt' }) =>
      nodesApi.coreUpdate(nodeId, { core }),
    onMutate: (vars) => setUpdatingCoreKey(`${vars.nodeId}:${vars.core}`),
    onSuccess: (res, vars) => {
      qc.invalidateQueries({ queryKey: ['nodes'] });
      qc.invalidateQueries({ queryKey: ['node', vars.nodeId] });
      toast({
        title: isRu
          ? `${vars.core}: обновлено${res.version ? ` до ${res.version}` : ''}`
          : `${vars.core} updated${res.version ? ` to ${res.version}` : ''}`,
        description: res.sha256 ? `SHA-256: ${res.sha256.slice(0, 16)}...` : res.message,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка обновления ядра' : 'Core update failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
    onSettled: () => setUpdatingCoreKey(null),
  });

  return (
        <TabsContent value="cores">
          <Card>
            <CardHeader className="flex flex-wrap flex-row items-center justify-between gap-3">
              <div>
                <CardTitle>
                  {isRu ? 'Каталог и обновление ядер' : 'Core Auto-Update Catalog'}
                </CardTitle>
                <CardDescription>
                  {isRu
                    ? 'Обновление прокси-ядер на активных нодах с проверкой SHA-256.'
                    : 'SHA-256 verified proxy core updates across connected nodes.'}
                </CardDescription>
              </div>
              <Button size="sm" variant="outline" onClick={handleRefreshCores} loading={refreshingCores}>
                <RefreshCw className="h-3.5 w-3.5" />
                {isRu ? 'Проверить обновления' : 'Check Updates'}
              </Button>
            </CardHeader>
            <CardContent className="space-y-4">
              {coreCatalog.length > 0 && (
                <div className="grid gap-2 sm:grid-cols-2">
                  {coreCatalog.map((c) => (
                    <div key={c.core} className="rack-inset rounded-xl p-2 text-xs space-y-1">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-bold">{c.name || c.core.toUpperCase()}</span>
                        {c.releaseUrl && (
                          <a
                            href={c.releaseUrl}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
                          >
                            GitHub <ExternalLink className="h-3 w-3" />
                          </a>
                        )}
                      </div>
                      <div>
                        {isRu ? 'Последний релиз:' : 'Latest:'}{' '}
                        <span className="font-mono font-semibold text-emerald-400">
                          {c.latestVersion || (isRu ? 'недоступно' : 'unavailable')}
                        </span>
                      </div>
                      {c.sha256 && (
                        <div className="font-mono text-[11px] text-muted-foreground break-all">
                          SHA-256: {c.sha256}
                        </div>
                      )}
                      {c.error && (
                        <div className="text-[11px] text-amber-400">{c.error}</div>
                      )}
                    </div>
                  ))}
                </div>
              )}
              <div className="space-y-2.5">
                {nodes.map((n) => {
                  const nodeBusy = updatingCoreKey?.startsWith(`${n.id}:`) ?? false;
                  const coreButtons: Array<{ core: 'xray' | 'singbox' | 'naive' | 'mieru' | 'telemt'; label: string }> = [
                    { core: 'xray', label: 'Xray' },
                    { core: 'singbox', label: 'Sing-box' },
                    { core: 'naive', label: 'NaiveProxy' },
                    { core: 'mieru', label: 'Mieru' },
                    { core: 'telemt', label: 'telemt' },
                  ];
                  return (
                    <div
                      key={n.id}
                      className="node-blade relative flex flex-wrap items-center justify-between gap-3 overflow-hidden rounded-xl border border-border/80 p-2 pl-5 text-xs"
                    >
                      <div className="absolute inset-y-0 left-0 w-1 bg-emerald-400" aria-hidden="true" />
                      <div className="space-y-1">
                        <div className="flex items-center gap-2">
                          <span className="font-bold text-sm">{n.name}</span>
                          <Badge variant={n.status === 'ONLINE' ? 'success' : 'secondary'}>
                            {n.status}
                          </Badge>
                        </div>
                        <div className="font-mono text-[11px] text-muted-foreground">
                          Xray: {n.xrayVersion || '—'} · Sing-box: {n.singboxVersion || '—'} · Naive:{' '}
                          {n.naiveVersion || '—'} · Mieru: {n.mieruVersion || '—'} · telemt: {n.telemtVersion || '—'}
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {coreButtons.map(({ core, label }) => {
                          const key = `${n.id}:${core}`;
                          return (
                            <Button
                              key={core}
                              size="sm"
                              variant="outline"
                              loading={updatingCoreKey === key}
                              disabled={nodeBusy || n.status === 'OFFLINE'}
                              onClick={() => updateCoreMut.mutate({ nodeId: n.id, core })}
                            >
                              <Download className="h-3.5 w-3.5" />
                              {isRu ? `Обновить ${label}` : `Update ${label}`}
                            </Button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
  );
}
