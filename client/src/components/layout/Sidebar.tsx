import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import {
  Activity,
  BookOpen,
  ChevronDown,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Cpu,
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
import { BrandMark, DEFAULT_PANEL_NAME } from '@/components/branding/BrandLogo';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { nodesApi } from '@/lib/api';
import { cn } from '@/lib/core-utils';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';
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

function CollapsedTooltip({
  enabled,
  label,
  children,
}: {
  enabled: boolean;
  label: string;
  children: ReactNode;
}) {
  if (!enabled) return <>{children}</>;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent
        side="right"
        sideOffset={10}
        className="border-border/80 bg-popover/95 font-medium backdrop-blur-md"
      >
        {label}
      </TooltipContent>
    </Tooltip>
  );
}

export function Sidebar({ mobileOpen, onCloseMobile }: SidebarProps) {
  const { t } = useTranslation();
  const location = useLocation();
  const user = useAuthStore((s) => s.user);
  const branding = usePublicConfigStore((s) => s.branding);
  const healthVersion = usePublicConfigStore((s) => s.healthVersion);
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebarCollapsed = useUiStore((s) => s.toggleSidebarCollapsed);

  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: () => nodesApi.list(),
    enabled: Boolean(user),
    refetchInterval: 30_000,
    staleTime: 15_000,
  });

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

  const totalNodes = nodes.length;
  const onlineNodes = nodes.filter((n) => n.status === 'ONLINE').length;
  const clusterStatus = useMemo(() => {
    if (totalNodes === 0) {
      return {
        label: t('sidebar.clusterEmpty'),
        dot: 'bg-zinc-400',
        tint: 'border-border/70 bg-muted/30 text-muted-foreground',
        ratioClass: 'text-muted-foreground',
      };
    }
    if (onlineNodes === totalNodes) {
      return {
        label: t('sidebar.clusterOnline'),
        dot: 'bg-emerald-400 animate-pulse shadow-[0_0_8px_#34d399]',
        tint: 'border-emerald-500/25 bg-emerald-500/[0.06] text-emerald-700 dark:text-emerald-300 hover:border-emerald-500/40',
        ratioClass: 'text-emerald-600 dark:text-emerald-300',
      };
    }
    if (onlineNodes > 0) {
      return {
        label: t('sidebar.clusterDegraded'),
        dot: 'bg-amber-400 animate-pulse shadow-[0_0_8px_rgba(251,191,36,0.65)]',
        tint: 'border-amber-500/30 bg-amber-500/[0.06] text-amber-700 dark:text-amber-300 hover:border-amber-500/45',
        ratioClass: 'text-amber-600 dark:text-amber-300',
      };
    }
    return {
      label: t('sidebar.clusterOffline'),
      dot: 'bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.7)]',
      tint: 'border-red-500/35 bg-red-500/[0.07] text-red-700 dark:text-red-300 hover:border-red-500/50',
      ratioClass: 'text-red-600 dark:text-red-300',
    };
  }, [totalNodes, onlineNodes, t]);

  const panelName = branding.panelName || DEFAULT_PANEL_NAME;
  const versionStr = healthVersion ? `v${healthVersion.replace(/^v/i, '')}` : 'v4.0.0';
  const showSettings = isAllowed(['SUPER_ADMIN', 'ADMIN']);

  const renderNavItem = (
    to: string,
    label: string,
    Icon: React.ComponentType<{ className?: string }>,
    isCollapsed: boolean,
  ) => {
    const active = isPathActive(to);
    return (
      <CollapsedTooltip key={to} enabled={isCollapsed} label={label}>
        <Link
          to={to}
          onClick={onCloseMobile}
          aria-current={active ? 'page' : undefined}
          className={cn(
            'group relative flex items-center rounded-lg py-2 text-[13px] transition-all duration-150',
            isCollapsed ? 'justify-center px-2' : 'gap-2.5 px-3',
            active
              ? 'bg-gradient-to-r from-emerald-500/15 via-emerald-500/5 to-transparent font-semibold text-emerald-700 dark:text-emerald-300'
              : 'font-medium text-muted-foreground hover:bg-black/[0.04] dark:hover:bg-white/[0.04] hover:text-foreground',
          )}
        >
          {active && (
            <span
              aria-hidden="true"
              className="absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-r-full bg-emerald-400 shadow-[0_0_10px_#34d399]"
            />
          )}
          <span className="relative inline-flex shrink-0 items-center justify-center">
            {active && (
              <span
                aria-hidden="true"
                className="pointer-events-none absolute -inset-1.5 rounded-full bg-emerald-400/20 blur-[5px]"
              />
            )}
            <Icon
              className={cn(
                'relative h-4 w-4 shrink-0 transition-colors',
                active
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-muted-foreground group-hover:text-foreground',
              )}
            />
          </span>
          {!isCollapsed && (
            <span className="truncate transition-transform duration-150 group-hover:translate-x-[2px]">
              {label}
            </span>
          )}
        </Link>
      </CollapsedTooltip>
    );
  };

  const renderNavContent = (isCollapsed: boolean) => (
    <TooltipProvider delayDuration={120}>
      <div className="keeper-sidebar flex h-full flex-col border-r border-border/70 border-t border-t-white/60 dark:border-white/[0.06] dark:border-t-white/[0.08]">
        {/* Brand Header Zone with subtle radial/grid texture */}
        <div
          className={cn(
            'keeper-sidebar-brand flex h-[72px] shrink-0 items-center border-b border-border/70 dark:border-white/[0.06]',
            isCollapsed ? 'justify-center px-2' : 'justify-between px-3.5',
          )}
        >
          <CollapsedTooltip enabled={isCollapsed} label={`${panelName} (${versionStr})`}>
            <Link
              to="/"
              onClick={onCloseMobile}
              className="flex min-w-0 items-center gap-3 font-semibold tracking-tight"
            >
              <BrandMark
                logoUrl={branding.hasCustomLogo ? branding.logoUrl : null}
                panelName={panelName}
                size={isCollapsed ? 'sm' : 'md'}
              />
              {!isCollapsed && (
                <div className="min-w-0">
                  <div className="truncate text-base font-bold leading-tight tracking-tight text-foreground">
                    {panelName}
                  </div>
                  <div className="mt-1 inline-flex items-center gap-1 rounded border border-emerald-500/25 bg-emerald-500/10 px-1.5 py-[1px] font-mono text-[9.5px] font-semibold uppercase tracking-[0.12em] text-emerald-600 dark:text-emerald-300">
                    <span className="h-1 w-1 rounded-full bg-emerald-400" />
                    <span>CORE · {versionStr}</span>
                  </div>
                </div>
              )}
            </Link>
          </CollapsedTooltip>

          {!isCollapsed && (
            <button
              type="button"
              onClick={onCloseMobile}
              aria-label="Close navigation"
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-black/5 dark:hover:bg-white/[0.06] hover:text-foreground lg:hidden"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>

        {/* Navigation Groups */}
        <nav className={cn('no-scrollbar flex-1 overflow-y-auto py-3.5 space-y-4', isCollapsed ? 'px-2' : 'px-3')}>
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
                      className="flex w-full items-center gap-2 px-2 py-1 text-left transition-colors group"
                    >
                      <span className="shrink-0 font-mono text-[10px] font-semibold uppercase tracking-[0.14em] text-zinc-500 group-hover:text-zinc-300">
                        {t(group.titleKey)}
                      </span>
                      <span
                        aria-hidden="true"
                        className="h-px flex-1 bg-gradient-to-r from-border/90 dark:from-white/[0.09] to-transparent"
                      />
                      <span className="flex shrink-0 items-center gap-1 font-mono text-[10px] text-zinc-500">
                        <span>{visibleItems.length}</span>
                        {isOpen ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                      </span>
                    </button>
                  ) : (
                    <div className="flex items-center gap-2 px-2 py-1">
                      <span className="shrink-0 font-mono text-[10px] font-semibold uppercase tracking-[0.14em] text-zinc-500">
                        {t(group.titleKey)}
                      </span>
                      <span
                        aria-hidden="true"
                        className="h-px flex-1 bg-gradient-to-r from-border/90 dark:from-white/[0.09] to-transparent"
                      />
                    </div>
                  ))}

                {isCollapsed && isCollapsible && (
                  <div className="mx-2 my-1 border-t border-border/60 dark:border-white/[0.07]" />
                )}

                {isOpen &&
                  (isCollapsible && !isCollapsed ? (
                    <div className="relative ml-2.5 space-y-0.5 pl-2.5 before:pointer-events-none before:absolute before:inset-y-1 before:left-0 before:w-px before:bg-gradient-to-b before:from-emerald-500/35 before:via-border/75 dark:before:via-white/[0.08] before:to-transparent">
                      {visibleItems.map((item) => renderNavItem(item.to, t(item.labelKey), item.icon, false))}
                    </div>
                  ) : (
                    <div className="space-y-0.5">
                      {visibleItems.map((item) =>
                        renderNavItem(item.to, t(item.labelKey), item.icon, isCollapsed),
                      )}
                    </div>
                  ))}
              </div>
            );
          })}
        </nav>

        {/* Bottom Infrastructure Status, Settings & Compact Control Strip */}
        <div
          className={cn(
            'shrink-0 border-t border-border/70 dark:border-white/[0.06] space-y-2',
            isCollapsed ? 'p-2' : 'p-2.5',
          )}
        >
          {/* Live Cluster Status Widget */}
          <CollapsedTooltip
            enabled={isCollapsed}
            label={`${clusterStatus.label} · ${t('sidebar.nodesActive', {
              online: String(onlineNodes),
              total: String(totalNodes),
            })}`}
          >
            <Link
              to="/nodes"
              onClick={onCloseMobile}
              className={cn(
                'flex items-center rounded-lg border text-xs transition-colors',
                clusterStatus.tint,
                isCollapsed ? 'justify-center px-1.5 py-2' : 'justify-between px-2.5 py-2',
              )}
            >
              <span className="flex min-w-0 items-center gap-2">
                <span className={cn('h-2 w-2 shrink-0 rounded-full', clusterStatus.dot)} />
                {!isCollapsed && <span className="truncate font-medium">{clusterStatus.label}</span>}
              </span>
              {!isCollapsed ? (
                <span className={cn('shrink-0 font-mono text-[11px] font-semibold tabular-nums', clusterStatus.ratioClass)}>
                  {onlineNodes}/{totalNodes}
                </span>
              ) : (
                <span className="sr-only">
                  {onlineNodes}/{totalNodes}
                </span>
              )}
            </Link>
          </CollapsedTooltip>

          {/* Settings Item aligned seamlessly with other nav items */}
          {showSettings && (
            <div className="space-y-0.5">
              {renderNavItem('/settings', t('nav.settings'), Settings, isCollapsed)}
            </div>
          )}

          {/* Compact Utility Control Bar */}
          <div
            className={cn(
              'flex items-center border-t border-border/60 dark:border-white/[0.05] pt-2 text-[11px] text-muted-foreground',
              isCollapsed ? 'flex-col gap-1.5' : 'justify-between px-1.5',
            )}
          >
            {!isCollapsed && (
              <span data-testid="panel-version" className="font-mono text-[10px] tracking-wide text-zinc-500">
                {versionStr}
              </span>
            )}

            <div className={cn('flex items-center gap-1', isCollapsed && 'flex-col')}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <a
                    href="/api/docs"
                    target="_blank"
                    rel="noopener noreferrer"
                    aria-label={t('nav.apiDocs')}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-black/[0.05] dark:hover:bg-white/[0.06] hover:text-foreground"
                  >
                    <BookOpen className="h-3.5 w-3.5" />
                  </a>
                </TooltipTrigger>
                <TooltipContent side={isCollapsed ? 'right' : 'top'} sideOffset={8}>
                  {t('nav.apiDocs')}
                </TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    onClick={toggleSidebarCollapsed}
                    aria-label={collapsed ? t('ui.expandSidebar') : t('ui.collapseSidebar')}
                    className="hidden h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-black/[0.05] dark:hover:bg-white/[0.06] hover:text-foreground lg:inline-flex"
                  >
                    {collapsed ? <ChevronsRight className="h-3.5 w-3.5" /> : <ChevronsLeft className="h-3.5 w-3.5" />}
                  </button>
                </TooltipTrigger>
                <TooltipContent side={isCollapsed ? 'right' : 'top'} sideOffset={8}>
                  {collapsed ? t('ui.expandSidebar') : t('ui.collapseSidebar')}
                </TooltipContent>
              </Tooltip>
            </div>
          </div>
        </div>
      </div>
    </TooltipProvider>
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
