import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Download, HardDrive } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { RackEmptyState } from '@/components/ui/PagePrimitives';
import { TabsContent } from '@/components/ui/tabs';
import { backupApi } from '@/lib/api';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore } from '@/lib/store';
import { toast } from '@/hooks/use-toast';

export function SettingsBackupTab() {
  const qc = useQueryClient();
  const { language } = useTranslation();
  const isRu = language === 'ru';
  const isSuperAdmin = useAuthStore((s) => s.user?.role === 'SUPER_ADMIN');

  const { data: backupLogs = [] } = useQuery({
    queryKey: ['backup-logs'],
    queryFn: () => backupApi.logs(),
    enabled: isSuperAdmin,
  });

  const triggerBackupMut = useMutation({
    mutationFn: () => backupApi.trigger({ type: 'DATABASE', destination: 'local' }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['backup-logs'] });
      toast({ title: isRu ? 'Бэкап запущен' : 'Backup started', variant: 'success' });
    },
  });

  return (
        <TabsContent value="backup">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle>{isRu ? 'Последние резервные копии' : 'Recent Backups'}</CardTitle>
              <Button size="sm" onClick={() => triggerBackupMut.mutate()} loading={triggerBackupMut.isPending}>
                {isRu ? 'Создать бэкап БД' : 'Trigger Database Backup'}
              </Button>
            </CardHeader>
            <CardContent className="space-y-2">
              {backupLogs.length === 0 ? (
                <RackEmptyState
                  icon={HardDrive}
                  title={isRu ? 'Резервных копий пока нет' : 'No backups logged yet'}
                />
              ) : (
                backupLogs.map((b) => {
                  const isDone = b.status === 'SUCCESS' || b.status === 'COMPLETED';
                  return (
                    <div
                      key={b.id}
                      className="rack-inset flex items-center justify-between rounded-xl px-2.5 py-1.5 text-xs"
                    >
                      <div>
                        <span className="font-semibold">{b.type}</span> · {b.destination} ·{' '}
                        <Badge variant={isDone ? 'success' : b.status === 'FAILED' ? 'destructive' : 'secondary'}>
                          {b.status}
                        </Badge>
                      </div>
                      {isDone && (
                        <a
                          href={backupApi.downloadUrl(b.id)}
                          className="inline-flex items-center gap-1 text-primary hover:underline"
                        >
                          <Download className="h-3.5 w-3.5" /> {isRu ? 'Скачать' : 'Download'}
                        </a>
                      )}
                    </div>
                  );
                })
              )}
            </CardContent>
          </Card>
        </TabsContent>
  );
}
