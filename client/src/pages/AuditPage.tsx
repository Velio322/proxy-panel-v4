import React, { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, RefreshCw, ScrollText, Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHero, RackEmptyState } from '@/components/ui/PagePrimitives';
import { auditApi } from '@/lib/api';
import { formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';

function actionBadgeVariant(action: string) {
  const up = action.toUpperCase();
  if (up.includes('CREATE') || up.includes('ADD') || up.includes('ENABLE')) return 'success' as const;
  if (up.includes('DELETE') || up.includes('BAN') || up.includes('DROP')) return 'destructive' as const;
  if (up.includes('UPDATE') || up.includes('PUSH') || up.includes('RESTART')) return 'default' as const;
  return 'outline' as const;
}

export function AuditPage() {
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [resourceFilter, setResourceFilter] = useState<string>('ALL');
  const [actionFilter, setActionFilter] = useState<string>('ALL');
  const [startDate, setStartDate] = useState<string>('');
  const [endDate, setEndDate] = useState<string>('');
  const [page, setPage] = useState<number>(1);
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});

  const { data: resources = [] } = useQuery({
    queryKey: ['audit-resources'],
    queryFn: () => auditApi.resources(),
  });

  const { data: actions = [] } = useQuery({
    queryKey: ['audit-actions'],
    queryFn: () => auditApi.actions(),
  });

  const {
    data: auditData,
    isLoading,
    refetch,
  } = useQuery({
    queryKey: ['audit-logs', resourceFilter, actionFilter, startDate, endDate, page],
    queryFn: () =>
      auditApi.list({
        resource: resourceFilter !== 'ALL' ? resourceFilter : undefined,
        action: actionFilter !== 'ALL' ? actionFilter : undefined,
        startDate: startDate || undefined,
        endDate: endDate || undefined,
        page,
        limit: 30,
      }),
  });

  const logs = auditData?.logs ?? [];
  const total = auditData?.total ?? logs.length;
  const totalPages = auditData?.pages ?? Math.max(1, Math.ceil(total / 30));

  const kpis = useMemo(() => {
    const actors = new Set<string>();
    let mutations = 0;
    let destructive = 0;
    for (const l of logs) {
      actors.add(l.user?.username || 'system');
      const up = l.action.toUpperCase();
      if (up.includes('DELETE') || up.includes('BAN') || up.includes('DROP')) destructive += 1;
      else mutations += 1;
    }
    return { actors: actors.size, mutations, destructive };
  }, [logs]);

  const toggleExpand = (id: string) => {
    setExpandedIds((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Журнал аудита' : 'Audit Log'}
        subtitle={
          isRu
            ? 'Неизменяемый журнал действий администраторов по нодам, клиентам, протоколам и настройкам'
            : 'Immutable administrative audit trail across users, nodes, clients, inbounds, and settings.'
        }
        icon={<ScrollText className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {total} {isRu ? 'событий' : 'events'}
          </Badge>
        }
        actions={
          <Button variant="outline" size="sm" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4" />
            <span>{isRu ? 'Обновить' : 'Refresh'}</span>
          </Button>
        }
      />

      {/* Audit KPI chip strip */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-xl border border-border/80 bg-background/60 px-3 py-2 text-[11px]">
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <ScrollText className="h-3 w-3 text-emerald-400" />
          <span className="font-bold text-foreground">{total}</span>
          {isRu ? 'записей' : 'events'}
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          {isRu ? 'изменения/удаления (стр.):' : 'mutations/deletions (page):'}
          <span className="font-bold text-emerald-400">{kpis.mutations}</span>
          <span className="font-bold text-red-400">/ {kpis.destructive}</span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono tabular-nums text-muted-foreground">
          <Users className="h-3 w-3 text-cyan-400" />
          <span className="font-bold text-foreground">{kpis.actors}</span>
          {isRu ? 'операторов' : 'actors'}
        </span>
      </div>


      {/* Filter Bar */}
      <Card>
        <CardHeader className="py-2.5">
          <div className="flex flex-wrap items-center gap-2.5">
            <Select
              value={resourceFilter}
              onValueChange={(v) => {
                setResourceFilter(v);
                setPage(1);
              }}
            >
              <SelectTrigger className="w-44" aria-label="Filter by resource">
                <SelectValue placeholder={isRu ? 'Все ресурсы' : 'All Resources'} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="ALL">{isRu ? 'Все ресурсы' : 'All Resources'}</SelectItem>
                {resources.map((r) => (
                  <SelectItem key={r} value={r}>
                    {r}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select
              value={actionFilter}
              onValueChange={(v) => {
                setActionFilter(v);
                setPage(1);
              }}
            >
              <SelectTrigger className="w-44" aria-label="Filter by action">
                <SelectValue placeholder={isRu ? 'Все действия' : 'All Actions'} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="ALL">{isRu ? 'Все действия' : 'All Actions'}</SelectItem>
                {actions.map((a) => (
                  <SelectItem key={a} value={a}>
                    {a}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Input
              type="date"
              value={startDate}
              onChange={(e) => {
                setStartDate(e.target.value);
                setPage(1);
              }}
              className="w-40"
              aria-label="Start date"
            />

            <Input
              type="date"
              value={endDate}
              onChange={(e) => {
                setEndDate(e.target.value);
                setPage(1);
              }}
              className="w-40"
              aria-label="End date"
            />
          </div>
        </CardHeader>
      </Card>

      {isLoading ? (
        <Card className="p-3 space-y-2">
          <Skeleton className="h-9 w-full" />
          <Skeleton className="h-9 w-full" />
          <Skeleton className="h-9 w-full" />
        </Card>
      ) : logs.length === 0 ? (
        <RackEmptyState
          icon={ScrollText}
          title={isRu ? 'Записей аудита не найдено' : 'No audit events found'}
          description={
            isRu
              ? 'Попробуйте изменить фильтр ресурса, действия или диапазон дат.'
              : 'Try adjusting your resource, action, or date range filters.'
          }
        />
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full text-xs">
                <thead className="bg-muted/40">
                  <tr className="border-b text-left text-muted-foreground">
                    <th className="py-1.5 px-2 w-8"></th>
                    <th className="py-1.5 px-2">{isRu ? 'Время' : 'Timestamp'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Пользователь' : 'Actor'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Действие' : 'Action'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Ресурс' : 'Resource'}</th>
                    <th className="py-1.5 px-2">IP</th>
                  </tr>
                </thead>
                <tbody>
                  {logs.map((entry) => {
                    const expanded = Boolean(expandedIds[entry.id]);
                    const hasDetails =
                      entry.details && Object.keys(entry.details).length > 0;
                    return (
                      <React.Fragment key={entry.id}>
                        <tr className="border-b last:border-b-0 hover:bg-muted/20">
                          <td className="py-1.5 px-2">
                            {hasDetails && (
                              <button
                                type="button"
                                onClick={() => toggleExpand(entry.id)}
                                className="rounded p-0.5 hover:bg-accent"
                                aria-label="Toggle payload details"
                              >
                                {expanded ? (
                                  <ChevronDown className="h-3.5 w-3.5" />
                                ) : (
                                  <ChevronRight className="h-3.5 w-3.5" />
                                )}
                              </button>
                            )}
                          </td>
                          <td className="py-1.5 px-2 font-mono text-muted-foreground tabular-nums">
                            {formatDateTime(entry.createdAt, timezone)}
                          </td>
                          <td className="py-1.5 px-2 font-semibold">
                            {entry.user?.username || 'system'}
                            {entry.user?.role && (
                              <span className="ml-1.5 text-[10px] text-muted-foreground">
                                ({entry.user.role})
                              </span>
                            )}
                          </td>
                          <td className="py-1.5 px-2">
                            <Badge variant={actionBadgeVariant(entry.action)} className="font-mono text-[10px]">
                              {entry.action}
                            </Badge>
                          </td>
                          <td className="py-1.5 px-2 font-mono">
                            {entry.resource}
                            {entry.resourceId ? `:${entry.resourceId.slice(0, 12)}` : ''}
                          </td>
                          <td className="py-1.5 px-2 font-mono text-muted-foreground">
                            {entry.ip || '—'}
                          </td>
                        </tr>
                        {expanded && hasDetails && (
                          <tr className="border-b bg-muted/10">
                            <td colSpan={6} className="p-3">
                              <pre className="rack-inset max-h-48 overflow-auto rounded-lg p-2 font-mono text-[11px]">
                                {JSON.stringify(entry.details, null, 2)}
                              </pre>
                            </td>
                          </tr>
                        )}
                      </React.Fragment>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      {totalPages > 1 && (
        <div className="flex items-center justify-between text-xs">
          <span className="text-muted-foreground">
            {isRu ? `Страница ${page} из ${totalPages}` : `Page ${page} of ${totalPages}`}
          </span>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
            >
              {isRu ? 'Назад' : 'Previous'}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => p + 1)}
            >
              {isRu ? 'Далее' : 'Next'}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
