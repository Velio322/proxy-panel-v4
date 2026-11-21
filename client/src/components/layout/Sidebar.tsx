import { useEffect, useMemo, useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import {
  Activity,
  ChevronDown,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Cpu,
  ExternalLink,
  FileCode2,
  Globe,
  HardDrive,
  LayoutDashboard,
  Network,
  Receipt,
  ScrollText,
  Server,
  Settings,
  Shield,
  ShieldAlert,
  Sparkles,
  Terminal,
  UserCheck,
  Users,
  Waypoints,
  X,
} from 'lucide-react';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';
import { cn } from '@/lib/core-utils';
import type { UserRole } from '@/types';

interface NavItem {
  to: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  roles?: UserRole[];
}

interface NavGroup {
  id: string;
  titleKey: string;
  collapsible?: boolean;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    id: 'main',
    titleKey: 'nav.main',
    collapsible: false,
    items: [
      { to: '/', labelKey: 'nav.overview', icon: LayoutDashboard },
      { to: '/clients', labelKey: 'nav.clients', icon: Users },
      { to: '/nodes', labelKey: 'nav.nodes', icon: Server },
      { to: '/inbounds', labelKey: 'nav.inbounds', icon: Network },
    ],
  },
  {
    id: 'observability',
    titleKey: 'nav.observability',
    collapsible: true,
    items: [
      { to: '/monitoring', labelKey: 'nav.monitoring', icon: Cpu, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/sessions', labelKey: 'nav.sessions', icon: Activity, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/logs', labelKey: 'nav.logs', icon: Terminal, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/inspectors', labelKey: 'nav.inspectors', icon: ShieldAlert, roles: ['SUPER_ADMIN', 'ADMIN'] },
    ],
  },
  {
    id: 'advanced',
    titleKey: 'nav.advanced',
    collapsible: true,
    items: [
      { to: '/hosts', labelKey: 'nav.hosts', icon: Globe },
      { to: '/routing', labelKey: 'nav.routing', icon: Waypoints },
      { to: '/plans', labelKey: 'nav.plans', icon: Sparkles, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/resellers', labelKey: 'nav.resellers', icon: UserCheck, roles: ['SUPER_ADMIN', 'ADMIN', 'RESELLER'] },
      { to: '/templates', labelKey: 'nav.templates', icon: FileCode2, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/srr', labelKey: 'nav.srr', icon: Shield, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/infra-billing', labelKey: 'nav.billing', icon: Receipt, roles: ['SUPER_ADMIN', 'ADMIN'] },
    ],
  },
  {
    id: 'system',
    titleKey: 'nav.administration',
    collapsible: true,
    items: [
      { to: '/settings', labelKey: 'nav.settings', icon: Settings, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/users', labelKey: 'nav.users', icon: Users, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/backup', labelKey: 'nav.backup', icon: HardDrive, roles: ['SUPER_ADMIN', 'ADMIN'] },
      { to: '/config-transfer', labelKey: 'nav.configTransfer', icon: FileCode2, roles: ['SUPER_ADMIN'] },
      { to: '/audit', labelKey: 'nav.audit', icon: ScrollText, roles: ['SUPER_ADMIN', 'ADMIN'] },
    ],
  },
];

interface SidebarProps {
  mobileOpen: boolean;
  onCloseMobile: () => void;
}

export function Sidebar({ mobileOpen, onCloseMobile }: SidebarProps) {
  const { t } = useTranslation();
  const location = useLocation();
  const user = useAuthStore((s) => s.user);
  const companyName = useUiStore((s) => s.companyName);
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebarCollapsed = useUiStore((s) => s.toggleSidebarCollapsed);
  const healthVersion = usePublicConfigStore((s) => s.healthVersion);

  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});

  const isPathActive = (to: string) =>
    to === '/' ? location.pathname === '/' : location.pathname === to || location.pathname.startsWith(`${to}/`);

  const activeGroupId = useMemo(() => {
    for (const g of NAV_GROUPS) {
      if (g.collapsible && g.items.some((i) => isPathActive(i.to))) {
        return g.id;
      }
    }
    return null;
  }, [location.pathname]);

  useEffect(() => {
    if (activeGroupId) {
      setOpenGroups((prev) => (prev[activeGroupId] ? prev : { ...prev, [activeGroupId]: true }));
    }
  }, [activeGroupId]);

  const toggleGroup = (id: string) => {
    setOpenGroups((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const isAllowed = (roles?: UserRole[]) => {
    if (!roles || !roles.length) return true;
    return user ? roles.includes(user.role) : false;
  };

  const renderNavContent = (isCollapsed: boolean) => (
    <div className="flex h-full flex-col border-r border-border/80 bg-card/85">
      <div
        className={cn(
          'flex h-14 items-center border-b border-border/80',
          isCollapsed ? 'justify-center px-2' : 'justify-between px-4',
        )}
      >
        <Link
          to="/"
          className="flex items-center gap-2.5 font-semibold tracking-tight min-w-0"
          onClick={onCloseMobile}
          title={companyName || 'ProxyPanel'}
        >
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground text-xs font-bold shadow-sm shadow-primary/30">
            PP
          </span>
          {!isCollapsed && (
            <div className="min-w-0">
              <div className="truncate text-sm font-bold leading-tight">{companyName || 'ProxyPanel'}</div>
              <div className="text-[10px] font-mono text-primary/80 leading-tight">CONTROL</div>
            </div>
          )}
        </Link>
        {!isCollapsed && (
          <button
            type="button"
            onClick={onCloseMobile}
            aria-label="Close navigation"
            className="rounded-md p-1 text-muted-foreground hover:bg-accent lg:hidden"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      <nav className={cn('flex-1 overflow-y-auto py-4 space-y-4', isCollapsed ? 'px-2' : 'px-3')}>
        {NAV_GROUPS.map((group) => {
          const visibleItems = group.items.filter((item) => isAllowed(item.roles));
          if (visibleItems.length === 0) return null;
          const isCollapsible = Boolean(group.collapsible);
          const isOpen = !isCollapsible || Boolean(openGroups[group.id]) || isCollapsed;

          return (
            <div key={group.id} className="space-y-1">
              {!isCollapsed &&
                (isCollapsible ? (
                  <button
                    type="button"
                    onClick={() => toggleGroup(group.id)}
                    aria-expanded={isOpen}
                    className="flex w-full items-center justify-between rounded-md px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/80 transition-colors hover:bg-accent/50 hover:text-foreground"
                  >
                    <span>{t(group.titleKey)}</span>
                    <span className="flex items-center gap-1 font-mono text-[10px] opacity-75">
                      <span>{visibleItems.length}</span>
                      {isOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                    </span>
                  </button>
                ) : (
                  <div className="px-2.5 pb-0.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                    {t(group.titleKey)}
                  </div>
                ))}

              {isCollapsed && isCollapsible && <div className="my-1 border-t border-border/50" />}

              {isOpen &&
                visibleItems.map((item) => {
                  const Icon = item.icon;
                  const active = isPathActive(item.to);
                  const label = t(item.labelKey);
                  return (
                    <Link
                      key={item.to}
                      to={item.to}
                      onClick={onCloseMobile}
                      title={isCollapsed ? label : undefined}
                      className={cn(
                        'relative flex items-center rounded-lg py-2 text-sm font-medium transition-all',
                        isCollapsed ? 'justify-center px-2' : 'gap-2.5 px-2.5',
                        active
                          ? 'bg-primary/15 text-primary border border-primary/25 shadow-sm shadow-primary/5'
                          : 'border border-transparent text-muted-foreground hover:bg-accent/60 hover:text-foreground',
                      )}
                    >
                      {active && !isCollapsed && (
                        <span
                          aria-hidden="true"
                          className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-full bg-primary shadow-[0_0_8px_hsl(var(--primary)/0.65)]"
                        />
                      )}
                      <Icon className={cn('h-4 w-4 shrink-0', active && 'text-primary')} />
                      {!isCollapsed && <span className="truncate">{label}</span>}
                    </Link>
                  );
                })}
            </div>
          );
        })}
      </nav>

      <div className={cn('border-t border-border/80 space-y-2', isCollapsed ? 'p-2' : 'p-3')}>
        <div className={cn('flex items-center gap-1.5', isCollapsed && 'flex-col')}>
          <Link
            to="/settings"
            onClick={onCloseMobile}
            title={t('nav.settings')}
            className={cn(
              'flex flex-1 items-center rounded-lg border border-border/80 py-1.5 text-xs font-medium text-muted-foreground transition-colors hover:border-primary/40 hover:bg-accent/60 hover:text-foreground',
              isCollapsed ? 'w-full justify-center px-2' : 'gap-2 px-2.5',
              isPathActive('/settings') && 'border-primary/30 bg-primary/10 text-primary',
            )}
          >
            <Settings className="h-3.5 w-3.5 shrink-0" />
            {!isCollapsed && <span className="truncate">{t('nav.settings')}</span>}
          </Link>
          <a
            href="/api/docs"
            target="_blank"
            rel="noopener noreferrer"
            title={t('nav.apiDocs')}
            aria-label={t('nav.apiDocs')}
            className={cn(
              'flex items-center justify-center rounded-lg border border-border/80 p-1.5 text-muted-foreground transition-colors hover:border-primary/40 hover:bg-accent/60 hover:text-foreground',
              isCollapsed && 'w-full',
            )}
          >
            <ExternalLink className="h-3.5 w-3.5 shrink-0" />
          </a>
        </div>

        <div
          className={cn(
            'flex items-center text-[11px] text-muted-foreground',
            isCollapsed ? 'justify-center' : 'justify-between px-1',
          )}
        >
          {!isCollapsed && (
            <span data-testid="panel-version" className="font-mono">
              {healthVersion ? `v${healthVersion.replace(/^v/, '')}` : 'v4.0.0'}
            </span>
          )}
          <button
            type="button"
            onClick={toggleSidebarCollapsed}
            aria-label={collapsed ? t('ui.expandSidebar') : t('ui.collapseSidebar')}
            title={collapsed ? t('ui.expandSidebar') : t('ui.collapseSidebar')}
            className="hidden lg:inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            {collapsed ? <ChevronsRight className="h-3.5 w-3.5" /> : <ChevronsLeft className="h-3.5 w-3.5" />}
          </button>
        </div>
      </div>
    </div>
  );

  return (
    <>
      <aside
        className={cn(
          'sticky top-0 hidden h-[100dvh] shrink-0 transition-[width] duration-200 lg:block',
          collapsed ? 'w-[68px]' : 'w-64',
        )}
      >
        {renderNavContent(collapsed)}
      </aside>
      {mobileOpen && (
        <div className="fixed inset-0 z-50 flex lg:hidden">
          <div className="fixed inset-0 bg-black/75 backdrop-blur-sm" onClick={onCloseMobile} />
          <aside className="relative z-50 h-full w-64 max-w-[82vw]">
            {renderNavContent(false)}
          </aside>
        </div>
      )}
    </>
  );
}
