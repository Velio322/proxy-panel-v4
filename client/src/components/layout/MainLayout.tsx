import { Suspense, useMemo, useState } from 'react';
import { Link, Outlet, useLocation, useNavigate } from 'react-router-dom';
import {
  ChevronRight,
  LogOut,
  Menu,
  Moon,
  Search,
  Settings,
  Shield,
  Sun,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Sidebar } from '@/components/layout/Sidebar';
import { CommandPalette } from '@/components/layout/CommandPalette';
import { NotificationCenter } from '@/components/layout/NotificationCenter';
import { Skeleton } from '@/components/ui/skeleton';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';

const ROUTE_LABEL_KEYS: Record<string, string> = {
  nodes: 'nav.nodes',
  inbounds: 'nav.inbounds',
  hosts: 'nav.hosts',
  clients: 'nav.clients',
  routing: 'nav.routing',
  monitoring: 'nav.monitoring',
  sessions: 'nav.sessions',
  logs: 'nav.logs',
  inspectors: 'nav.inspectors',
  'srh-inspector': 'nav.inspectors',
  'hwid-inspector': 'nav.inspectors',
  'torrent-blocker': 'nav.inspectors',
  plans: 'nav.plans',
  resellers: 'nav.resellers',
  templates: 'nav.templates',
  srr: 'nav.srr',
  'infra-billing': 'nav.billing',
  users: 'nav.users',
  audit: 'nav.audit',
  backup: 'nav.backup',
  'config-transfer': 'nav.configTransfer',
  settings: 'nav.settings',
  traffic: 'clients.trafficChart',
};

function ContentFallback() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-8 w-52" />
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Skeleton className="h-24" />
        <Skeleton className="h-24" />
        <Skeleton className="h-24" />
        <Skeleton className="h-24" />
      </div>
      <Skeleton className="h-64" />
    </div>
  );
}

export function MainLayout() {
  const { t, language, setLanguage } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const healthStatus = usePublicConfigStore((s) => s.healthStatus);

  const [mobileOpen, setMobileOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);

  const breadcrumbs = useMemo(() => {
    const parts = location.pathname.split('/').filter(Boolean);
    if (parts.length === 0) return [{ path: '/', label: t('nav.overview') }];
    return parts.map((seg, idx) => {
      const path = `/${parts.slice(0, idx + 1).join('/')}`;
      const key = ROUTE_LABEL_KEYS[seg];
      const label = key ? t(key) : seg.length > 14 ? `${seg.slice(0, 12)}…` : seg;
      return { path, label };
    });
  }, [location.pathname, t]);

  const handleLogout = async () => {
    await logout();
    navigate('/login');
  };

  return (
    <div className="flex min-h-screen overflow-x-clip bg-background text-foreground app-shell-bg">
      <Sidebar mobileOpen={mobileOpen} onCloseMobile={() => setMobileOpen(false)} />

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-30 flex h-14 items-center justify-between gap-2 border-b border-border/80 bg-background/85 px-3 backdrop-blur-md sm:px-5">
          <div className="flex min-w-0 items-center gap-2.5">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="lg:hidden shrink-0"
              onClick={() => setMobileOpen(true)}
              aria-label="Open navigation"
            >
              <Menu className="h-5 w-5" />
            </Button>

            <nav aria-label="Breadcrumb" className="hidden sm:flex items-center gap-1.5 text-xs text-muted-foreground pr-2">
              {breadcrumbs.map((crumb, idx) => (
                <span key={crumb.path} className="flex items-center gap-1.5">
                  {idx > 0 && <ChevronRight className="h-3.5 w-3.5 opacity-50" />}
                  {idx === breadcrumbs.length - 1 ? (
                    <span className="font-semibold text-foreground truncate max-w-[180px]">{crumb.label}</span>
                  ) : (
                    <Link to={crumb.path} className="hover:text-primary transition-colors truncate max-w-[140px]">
                      {crumb.label}
                    </Link>
                  )}
                </span>
              ))}
            </nav>

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setCommandOpen(true)}
              aria-label={t('common.search')}
              className="h-8 w-8 p-0 justify-center sm:w-52 md:w-64 sm:justify-between sm:px-3 text-muted-foreground"
            >
              <span className="flex items-center gap-2 truncate">
                <Search className="h-3.5 w-3.5 shrink-0" />
                <span className="hidden sm:inline truncate text-xs">{t('common.search')}</span>
              </span>
              <kbd className="hidden rounded border border-border/80 bg-muted px-1.5 py-0.5 text-[10px] font-mono sm:inline-block">
                Ctrl+K
              </kbd>
            </Button>
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            <span
              data-testid="api-health-pill"
              title={`API: ${healthStatus}`}
              className="hidden md:inline-flex items-center gap-1.5 rounded-full border border-border/80 bg-card/80 px-2.5 py-1 text-[11px] font-medium text-muted-foreground"
            >
              <span
                className={
                  healthStatus === 'ok'
                    ? 'h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_8px_hsl(162_84%_43%/0.8)]'
                    : healthStatus === 'degraded'
                      ? 'h-2 w-2 rounded-full bg-amber-400'
                      : 'h-2 w-2 rounded-full bg-slate-400'
                }
              />
              <span>
                {healthStatus === 'ok'
                  ? t('health.ok')
                  : healthStatus === 'degraded'
                    ? t('health.degraded')
                    : t('health.checking')}
              </span>
            </span>

            <NotificationCenter />

            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => setLanguage(language === 'en' ? 'ru' : 'en')}
              className="h-8 px-2.5 text-xs font-semibold uppercase"
              aria-label="Switch language"
            >
              {language}
            </Button>

            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={toggleTheme}
              aria-label="Toggle theme"
            >
              {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm" className="h-8 gap-2 px-2.5">
                  <span className="hidden sm:inline font-medium">{user?.username}</span>
                  <Badge variant="default" className="px-1.5 py-0 text-[10px]">
                    {user?.role}
                  </Badge>
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-52">
                <DropdownMenuLabel>{user?.username}</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => navigate('/settings')}>
                  <Settings className="h-4 w-4" />
                  <span>{t('nav.settings')}</span>
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => navigate('/settings?tab=security')}>
                  <Shield className="h-4 w-4" />
                  <span>{t('settings.security')}</span>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={handleLogout}
                  className="text-red-600 dark:text-red-400 focus:text-red-600 dark:focus:text-red-400"
                >
                  <LogOut className="h-4 w-4" />
                  <span>{t('auth.logout')}</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </header>

        <main className="flex-1 p-4 md:p-6">
          <div className="mx-auto w-full max-w-[1440px]">
            <Suspense fallback={<ContentFallback />}>
              <Outlet />
            </Suspense>
          </div>
        </main>
      </div>

      <CommandPalette open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
