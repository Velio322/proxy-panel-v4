import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Copy, Download, HardDrive, Play, RefreshCw, Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { PageHero, RackEmptyState, SplitBar } from '@/components/ui/PagePrimitives';
import { backupApi, getApiErrorMessage } from '@/lib/api';
import { formatBytes, formatDateTime } from '@/lib/core-utils';
import { useUiStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

const BACKUP_TYPES = ['DATABASE', 'CONFIG', 'FULL'] as const;
const BACKUP_DESTINATIONS = ['local'] as const;

const RESTORE_CMD = 'pg_restore --clean --if-exists --dbname="$DATABASE_URL" backup.dump';

export function BackupPage() {
  const qc = useQueryClient();
  const timezone = useUiStore((s) => s.timezone);
  const language = useUiStore((s) => s.language);
  const isRu = language === 'ru';

  const [type, setType] = useState<string>('DATABASE');
  const [destination, setDestination] = useState<string>('local');
  const [copiedCmd, setCopiedCmd] = useState(false);

  const {
    data: logs = [],
    isLoading,
    refetch,
  } = useQuery({
    queryKey: ['backup-logs'],
    queryFn: () => backupApi.logs(),
    refetchInterval: (query) => {
      const items = query.state.data ?? [];
      const hasActive = items.some((b) => b.status === 'IN_PROGRESS' || b.status === 'PENDING');
      return hasActive ? 3000 : false;
    },
  });

  const { data: config = {} } = useQuery({
    queryKey: ['backup-config'],
    queryFn: () => backupApi.config(),
  });

  const stats = useMemo(() => {
    let ok = 0;
    let failed = 0;
    let totalBytes = 0;
    for (const l of logs) {
      if (l.status === 'SUCCESS' || l.status === 'COMPLETED') ok += 1;
      else if (l.status === 'FAILED') failed += 1;
      totalBytes += Number(l.sizeBytes ?? l.fileSize ?? 0);
    }
    return {
      total: logs.length,
      ok,
      failed,
      totalBytes,
      latest: logs[0]?.createdAt ?? null,
    };
  }, [logs]);

  const triggerMut = useMutation({
    mutationFn: () => backupApi.trigger({ type, destination }),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ['backup-logs'] });
      toast({
        title: isRu ? 'Резервное копирование запущено' : 'Backup triggered',
        description: `Job ${res.id} (${res.status})`,
        variant: 'success',
      });
    },
    onError: (err) =>
      toast({
        title: isRu ? 'Ошибка запуска бэкапа' : 'Trigger backup failed',
        description: getApiErrorMessage(err),
        variant: 'destructive',
      }),
  });

  const copyRestoreCmd = async () => {
    try {
      await navigator.clipboard.writeText(RESTORE_CMD);
      setCopiedCmd(true);
      window.setTimeout(() => setCopiedCmd(false), 1500);
    } catch {
      // clipboard unavailable
    }
  };

  return (
    <div className="space-y-5">
      <PageHero
        title={isRu ? 'Резервные копии и снапшоты' : 'Backups & Snapshots'}
        subtitle={
          isRu
            ? 'Создание дампов базы данных PostgreSQL и конфигурации панели с прямым скачиванием'
            : 'Create database and configuration snapshots and stream encrypted archives locally.'
        }
        icon={<HardDrive className="h-4 w-4" />}
        badge={
          <Badge variant="outline" className="font-mono text-xs tabular-nums">
            {stats.ok}/{stats.total}
          </Badge>
        }
        actions={
          <Button variant="outline" size="sm" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4" />
            <span>{isRu ? 'Обновить' : 'Refresh'}</span>
          </Button>
        }
      />

      {/* 3-Card KPI Ribbon */}
      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-3 space-y-2">
          <div className="text-xs text-muted-foreground">{isRu ? 'Успешные копии' : 'Successful Snapshots'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {stats.ok} / {stats.total}
          </div>
          <SplitBar
            segments={[
              { pct: stats.total > 0 ? (stats.ok / stats.total) * 100 : 100, className: 'bg-emerald-400' },
              { pct: stats.total > 0 ? (stats.failed / stats.total) * 100 : 0, className: 'bg-red-500' },
            ]}
          />
        </Card>

        <Card className="p-3 space-y-1.5">
          <div className="text-xs text-muted-foreground">{isRu ? 'Суммарный объём архивов' : 'Total Archive Size'}</div>
          <div className="font-mono text-2xl font-bold tabular-nums">
            {formatBytes(stats.totalBytes, { locale: language })}
          </div>
        </Card>

        <Card className="p-3 space-y-1.5">
          <div className="text-xs text-muted-foreground">{isRu ? 'Последний бэкап' : 'Latest Snapshot'}</div>
          <div className="font-mono text-sm font-bold">
            {stats.latest ? formatDateTime(stats.latest, timezone) : '—'}
          </div>
        </Card>
      </div>

      <div className="grid gap-3 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center gap-2 text-sm">
              <HardDrive className="h-4 w-4 text-primary" />
              <span>{isRu ? 'Создать резервную копию вручную' : 'Trigger Manual Backup'}</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-2 sm:grid-cols-3">
              <div className="space-y-1.5">
                <Label>{isRu ? 'Тип копии' : 'Backup Scope'}</Label>
                <Select value={type} onValueChange={setType}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {BACKUP_TYPES.map((t) => (
                      <SelectItem key={t} value={t}>
                        {t}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-1.5">
                <Label>{isRu ? 'Хранилище' : 'Destination'}</Label>
                <Select value={destination} onValueChange={setDestination}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {BACKUP_DESTINATIONS.map((d) => (
                      <SelectItem key={d} value={d}>
                        {d.toUpperCase()}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-end">
                <Button
                  className="w-full"
                  onClick={() => triggerMut.mutate()}
                  loading={triggerMut.isPending}
                >
                  <Play className="h-4 w-4" />
                  <span>{isRu ? 'Запустить бэкап' : 'Trigger Backup'}</span>
                </Button>
              </div>
            </div>

            {Object.keys(config).length > 0 && (
              <div className="rack-inset rounded-lg p-3 text-xs text-muted-foreground">
                <span className="font-semibold text-foreground">
                  {isRu ? 'Расписание:' : 'Configured Schedule:'}
                </span>{' '}
                {String(config.schedule || config.cron || (isRu ? 'Вручную / По умолчанию' : 'Manual / Default'))}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center gap-2 text-sm">
              <Terminal className="h-4 w-4 text-primary" />
              <span>{isRu ? 'Восстановление PostgreSQL' : 'Schedule & PostgreSQL Restore Guide'}</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs text-muted-foreground">
            <div className="rack-inset flex items-center justify-between rounded-md px-2.5 py-2">
              <span>{isRu ? 'Хранение / Папка' : 'Retention / Dir'}</span>
              <span className="font-mono text-foreground">
                {String(config.retentionDays ?? 7)}d · {String(config.dir || 'local')}
              </span>
            </div>
            <p>
              {isRu
                ? 'Для безопасного восстановления скачанного дампа БД остановите сервис панели и выполните:'
                : 'To restore a downloaded database snapshot safely without schema drift, stop the panel worker and run:'}
            </p>
            <div className="rack-inset flex items-center justify-between gap-2 rounded-md px-2.5 py-2 font-mono text-[11px] text-foreground">
              <span className="break-words min-w-0">{RESTORE_CMD}</span>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="h-6 w-6 shrink-0"
                onClick={copyRestoreCmd}
                aria-label="Copy restore command"
              >
                {copiedCmd ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>

      {isLoading ? (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          {isRu ? 'Загрузка истории бэкапов...' : 'Loading backup logs...'}
        </Card>
      ) : logs.length === 0 ? (
        <RackEmptyState
          icon={HardDrive}
          title={isRu ? 'Резервные копии ещё не создавались' : 'No backup snapshots recorded yet'}
          description={
            isRu
              ? 'Нажмите «Запустить бэкап» выше, чтобы создать первый снапшот базы данных.'
              : 'Click "Trigger Backup" above to create your first database snapshot.'
          }
          action={
            <Button size="sm" onClick={() => triggerMut.mutate()} loading={triggerMut.isPending}>
              <Play className="h-4 w-4" />
              <span>{isRu ? 'Запустить бэкап' : 'Trigger Backup'}</span>
            </Button>
          }
        />
      ) : (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">
              {isRu ? `История резервных копий (${logs.length})` : `Backup History (${logs.length})`}
            </CardTitle>
          </CardHeader>
          <CardContent className="p-0">
            <div className="overflow-x-auto">
              <table className="data-table w-full text-xs">
                <thead className="bg-muted/40">
                  <tr className="border-b text-left text-muted-foreground">
                    <th className="py-1.5 px-2">{isRu ? 'Дата создания' : 'Created At'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Тип' : 'Type'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Хранилище' : 'Destination'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Файл' : 'Filename'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Размер' : 'Size'}</th>
                    <th className="py-1.5 px-2">{isRu ? 'Статус' : 'Status'}</th>
                    <th className="py-1.5 px-2 text-right">{isRu ? 'Скачать' : 'Download'}</th>
                  </tr>
                </thead>
                <tbody>
                  {logs.map((log) => {
                    const isDone = log.status === 'SUCCESS' || log.status === 'COMPLETED';
                    return (
                      <tr key={log.id} className="border-b last:border-b-0 hover:bg-muted/20">
                        <td className="py-1.5 px-2 font-mono text-muted-foreground tabular-nums">
                          {formatDateTime(log.createdAt, timezone)}
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge variant="outline">{log.type}</Badge>
                        </td>
                        <td className="py-1.5 px-2 uppercase font-mono">{log.destination}</td>
                        <td className="py-1.5 px-2 font-mono">{log.filename || log.id}</td>
                        <td className="py-1.5 px-2 font-mono tabular-nums">
                          {log.sizeBytes != null ? formatBytes(log.sizeBytes, { locale: language }) : '—'}
                        </td>
                        <td className="py-1.5 px-2">
                          <Badge
                            variant={
                              isDone ? 'success' : log.status === 'FAILED' ? 'destructive' : 'warning'
                            }
                          >
                            {log.status}
                          </Badge>
                          {log.error && (
                            <div className="mt-0.5 text-[11px] text-red-500 dark:text-red-400">
                              {log.error}
                            </div>
                          )}
                        </td>
                        <td className="py-1.5 px-2 text-right">
                          {isDone && (
                            <Button size="sm" variant="outline" asChild>
                              <a
                                href={backupApi.downloadUrl(log.id)}
                                download={log.filename || `backup-${log.id}.tar.gz`}
                              >
                                <Download className="h-3.5 w-3.5" />
                                <span>{isRu ? 'Скачать' : 'Stream Download'}</span>
                              </a>
                            </Button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
