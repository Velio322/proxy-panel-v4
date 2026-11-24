import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Activity, ArrowRight, Server, Signal, Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { NodeLogViewer } from '@/components/nodes/NodeLogViewer';
import { latencyBadgeClass } from '@/components/nodes/NodeTelemetryPrimitives';
import { nodesApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';

export function NodeLogsPage() {
  const { id: paramNodeId } = useParams<{ id?: string }>();
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';
  const [selectedNodeId, setSelectedNodeId] = useState<string>(paramNodeId || '');

  const { data: nodes = [], isLoading } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
  });

  useEffect(() => {
    if (paramNodeId) {
      setSelectedNodeId(paramNodeId);
    } else if (!selectedNodeId && nodes.length > 0) {
      setSelectedNodeId(nodes[0].id);
    }
  }, [paramNodeId, nodes, selectedNodeId]);

  const activeNode = nodes.find((n) => n.id === selectedNodeId);

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Потоковые логи нод' : 'Live Node Logs'}
        subtitle={
          isRu
            ? 'Потоковый терминал и журнал событий в реальном времени для Xray, Sing-box, NaiveProxy, Mieru и агента ноды'
            : 'Real-time stream and polling viewer for Xray, Sing-box, NaiveProxy, Mieru, and Node Agent logs.'
        }
        icon={<Terminal className="h-4 w-4" />}
        badge={
          activeNode ? (
            <Badge variant={activeNode.status === 'ONLINE' ? 'success' : 'secondary'}>
              {activeNode.name} · {activeNode.status}
            </Badge>
          ) : undefined
        }
        actions={
          <>
            <Button variant="outline" size="sm" asChild>
              <Link to="/monitoring">
                <Activity className="h-4 w-4" />
                <span>{isRu ? 'Мониторинг' : 'Monitoring'}</span>
              </Link>
            </Button>
            {activeNode && (
              <Button size="sm" variant="secondary" asChild>
                <Link to={`/nodes/${activeNode.id}`}>
                  <span>{isRu ? 'Консоль ноды' : 'Node Console'}</span>
                  <ArrowRight className="h-3.5 w-3.5" />
                </Link>
              </Button>
            )}
          </>
        }
      />

      {/* Interactive Node Selector Strip */}
      {nodes.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          {nodes.map((n) => {
            const selected = n.id === selectedNodeId;
            return (
              <button
                key={n.id}
                type="button"
                onClick={() => setSelectedNodeId(n.id)}
                className={cn(
                  'node-blade relative flex items-center gap-2.5 overflow-hidden rounded-xl border px-3.5 py-2 text-xs transition-all',
                  selected
                    ? 'border-primary/60 bg-primary/15 text-foreground ring-1 ring-primary/25'
                    : 'border-border/80 text-muted-foreground hover:border-border hover:text-foreground',
                )}
              >
                <span
                  className={cn(
                    'h-2 w-2 rounded-full',
                    n.status === 'ONLINE'
                      ? 'bg-emerald-400 animate-pulse'
                      : n.status === 'MAINTENANCE'
                        ? 'bg-amber-400'
                        : 'bg-zinc-500',
                  )}
                />
                <span className="font-bold">{n.name}</span>
                <span className="font-mono text-[11px] opacity-75">
                  {n.host}:{n.port}
                </span>
                {n.lastPingMs != null && (
                  <span
                    className={cn(
                      'inline-flex items-center gap-1 rounded border px-1.5 py-0 font-mono text-[10px]',
                      latencyBadgeClass(n.lastPingMs),
                    )}
                  >
                    <Signal className="h-2.5 w-2.5" />
                    {n.lastPingMs}ms
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}

      {isLoading ? (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка списка нод...' : 'Loading nodes...'}
        </Card>
      ) : !selectedNodeId ? (
        <RackEmptyState
          icon={Server}
          title={isRu ? 'Нет доступных нод для просмотра логов' : 'No nodes available to inspect logs'}
          description={
            isRu
              ? 'Добавьте хотя бы одну серверную ноду в разделе «Ноды».'
              : 'Add at least one proxy node to stream live logs.'
          }
        />
      ) : (
        <Card>
          <CardHeader className="pb-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <CardTitle className="flex items-center gap-2 text-sm">
                <Terminal className="h-4 w-4 text-primary" />
                <span>
                  {activeNode ? `${activeNode.name} (${activeNode.host})` : isRu ? 'Поток логов' : 'Log Stream'}
                </span>
              </CardTitle>
              {activeNode && (
                <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-muted-foreground">
                  <span>
                    CPU {activeNode.cpuUsage != null ? `${activeNode.cpuUsage.toFixed(0)}%` : '—'}
                  </span>
                  <span>·</span>
                  <span>
                    RAM {activeNode.memUsage != null ? `${activeNode.memUsage.toFixed(0)}%` : '—'}
                  </span>
                  <Badge variant={activeNode.status === 'ONLINE' ? 'success' : 'secondary'}>
                    {activeNode.status}
                  </Badge>
                </div>
              )}
            </div>
          </CardHeader>
          <CardContent>
            <NodeLogViewer nodeId={selectedNodeId} />
          </CardContent>
        </Card>
      )}
    </div>
  );
}
