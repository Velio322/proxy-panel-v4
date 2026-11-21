import { useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  AlertCircle,
  AlertTriangle,
  Bell,
  CheckCheck,
  CheckCircle2,
  Clock,
  Gauge,
  Info,
  ServerCrash,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { notificationsApi } from '@/lib/api';
import { formatDateTime } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, useUiStore } from '@/lib/store';
import type { NotificationItem, NotificationType } from '@/types';

function NotificationIcon({ type }: { type: NotificationType }) {
  switch (type) {
    case 'NODE_OFFLINE':
      return <ServerCrash className="h-4 w-4 shrink-0 text-red-400" />;
    case 'TRAFFIC_LIMIT':
      return <Gauge className="h-4 w-4 shrink-0 text-amber-400" />;
    case 'SUBSCRIPTION_EXPIRED':
      return <Clock className="h-4 w-4 shrink-0 text-amber-400" />;
    case 'ERROR':
      return <AlertCircle className="h-4 w-4 shrink-0 text-red-400" />;
    case 'WARNING':
      return <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />;
    case 'SUCCESS':
    case 'PAYMENT_RECEIVED':
      return <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />;
    default:
      return <Info className="h-4 w-4 shrink-0 text-sky-400" />;
  }
}

export function NotificationCenter() {
  const { t } = useTranslation();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const timezone = useUiStore((s) => s.timezone);
  const qc = useQueryClient();

  const { data, refetch } = useQuery({
    queryKey: ['notifications'],
    queryFn: () => notificationsApi.list(),
    enabled: isAuthenticated,
    refetchInterval: 60_000,
    retry: false,
  });

  useEffect(() => {
    if (isAuthenticated) {
      refetch();
    }
  }, [isAuthenticated, refetch]);

  const markReadMut = useMutation({
    mutationFn: (id: string) => notificationsApi.markRead(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['notifications'] }),
  });

  const markAllMut = useMutation({
    mutationFn: () => notificationsApi.markAllRead(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['notifications'] }),
  });

  const notifications = (data?.notifications ?? []).slice(0, 50);
  const unreadCount = data?.unreadCount ?? 0;

  return (
    <DropdownMenu onOpenChange={(open) => open && refetch()}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" className="relative" aria-label={t('notifications.title')}>
          <Bell className="h-4 w-4" />
          {unreadCount > 0 && (
            <span
              data-testid="notification-badge"
              className="absolute -right-0.5 -top-0.5 flex h-4 min-w-[1rem] items-center justify-center rounded-full bg-red-500 px-1 text-[10px] font-bold text-white"
            >
              {unreadCount > 99 ? '99+' : unreadCount}
            </span>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80 p-0">
        <div className="flex items-center justify-between px-3 py-2">
          <DropdownMenuLabel className="p-0 text-sm text-foreground">
            {t('notifications.title')}
          </DropdownMenuLabel>
          {unreadCount > 0 && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() => markAllMut.mutate()}
              disabled={markAllMut.isPending}
            >
              <CheckCheck className="h-3.5 w-3.5" />
              {t('notifications.markAllRead')}
            </Button>
          )}
        </div>
        <DropdownMenuSeparator className="my-0" />
        <div className="max-h-80 overflow-y-auto">
          {notifications.length === 0 ? (
            <div className="py-8 text-center text-xs text-muted-foreground">
              {t('notifications.empty')}
            </div>
          ) : (
            notifications.map((item: NotificationItem) => (
              <button
                key={item.id}
                type="button"
                onClick={() => !item.read && markReadMut.mutate(item.id)}
                className={`flex w-full items-start gap-2.5 border-b px-3 py-2.5 text-left transition-colors last:border-b-0 hover:bg-accent/50 ${
                  !item.read ? 'bg-primary/5' : ''
                }`}
              >
                <div className="mt-0.5">
                  <NotificationIcon type={item.type} />
                </div>
                <div className="min-w-0 flex-1 space-y-0.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-xs font-semibold">{item.title}</span>
                    {!item.read && <span className="h-2 w-2 shrink-0 rounded-full bg-primary" />}
                  </div>
                  <p className="line-clamp-2 text-xs text-muted-foreground">{item.message}</p>
                  <p className="text-[11px] text-muted-foreground/80">
                    {formatDateTime(item.createdAt, timezone)}
                  </p>
                </div>
              </button>
            ))
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
