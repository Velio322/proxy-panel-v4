import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Activity,
  ArrowRight,
  Cpu,
  FileCode2,
  Globe,
  HardDrive,
  Network,
  Receipt,
  Search,
  Server,
  Settings,
  Shield,
  Sparkles,
  Terminal,
  Users,
  Waypoints,
} from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { clientsApi, inboundsApi, nodesApi } from '@/lib/api';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, useUiStore } from '@/lib/store';
import { cn } from '@/lib/core-utils';
import type { Client, Inbound, NodeItem, UserRole } from '@/types';

interface PaletteEntry {
  id: string;
  group: 'pages' | 'actions' | 'clients' | 'nodes' | 'inbounds';
  title: string;
  subtitle?: string;
  icon: React.ComponentType<{ className?: string }>;
  onSelect: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const ADMIN_ROLES: UserRole[] = ['SUPER_ADMIN', 'ADMIN'];

export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const user = useAuthStore((s) => s.user);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const toggleDensity = useUiStore((s) => s.toggleDensity);
  const toggleSidebarCollapsed = useUiStore((s) => s.toggleSidebarCollapsed);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const language = useUiStore((s) => s.language);

  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [clients, setClients] = useState<Client[]>([]);
  const [nodes, setNodes] = useState<NodeItem[]>([]);
  const [inbounds, setInbounds] = useState<Inbound[]>([]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        onOpenChange(!open);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, onOpenChange]);

  useEffect(() => {
    if (!open) {
      setQuery('');
      setClients([]);
      setNodes([]);
      setInbounds([]);
      setSelectedIndex(0);
      return;
    }
  }, [open]);

  useEffect(() => {
    const q = query.trim();
    if (!open || q.length < 2) {
      setClients([]);
      setNodes([]);
      setInbounds([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      const [cRes, nRes, iRes] = await Promise.allSettled([
        clientsApi.list({ search: q, limit: 5 }),
        nodesApi.list({ search: q }),
        inboundsApi.list({ search: q }),
      ]);
      if (cancelled) return;
      setClients(cRes.status === 'fulfilled' ? cRes.value.data : []);
      setNodes(nRes.status === 'fulfilled' ? nRes.value.slice(0, 5) : []);
      setInbounds(iRes.status === 'fulfilled' ? iRes.value.slice(0, 5) : []);
    }, 220);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [open, query]);

  const isAdmin = user ? ADMIN_ROLES.includes(user.role) : false;
  const isSuperAdmin = user?.role === 'SUPER_ADMIN';

  const baseEntries = useMemo<PaletteEntry[]>(() => {
    const go = (path: string) => () => {
      navigate(path);
      onOpenChange(false);
    };

    const pages: PaletteEntry[] = [
      { id: 'p-home', group: 'pages', title: t('nav.overview'), icon: Activity, onSelect: go('/') },
      { id: 'p-nodes', group: 'pages', title: t('nav.nodes'), icon: Server, onSelect: go('/nodes') },
      { id: 'p-clients', group: 'pages', title: t('nav.clients'), icon: Users, onSelect: go('/clients') },
      { id: 'p-inbounds', group: 'pages', title: t('nav.inbounds'), icon: Network, onSelect: go('/inbounds') },
      { id: 'p-hosts', group: 'pages', title: t('nav.hosts'), icon: Globe, onSelect: go('/hosts') },
      { id: 'p-routing', group: 'pages', title: t('nav.routing'), icon: Waypoints, onSelect: go('/routing') },
    ];

    if (isAdmin) {
      pages.push(
        { id: 'p-mon', group: 'pages', title: t('nav.monitoring'), icon: Cpu, onSelect: go('/monitoring') },
        { id: 'p-sess', group: 'pages', title: t('nav.sessions'), icon: Terminal, onSelect: go('/sessions') },
        { id: 'p-logs', group: 'pages', title: t('nav.logs'), icon: Terminal, onSelect: go('/logs') },
        { id: 'p-insp', group: 'pages', title: t('nav.inspectors'), icon: Shield, onSelect: go('/inspectors') },
        { id: 'p-plans', group: 'pages', title: t('nav.plans'), icon: Sparkles, onSelect: go('/plans') },
        { id: 'p-resellers', group: 'pages', title: t('nav.resellers'), icon: Users, onSelect: go('/resellers') },
        { id: 'p-tmpl', group: 'pages', title: t('nav.templates'), icon: FileCode2, onSelect: go('/templates') },
        { id: 'p-srr', group: 'pages', title: t('nav.srr'), icon: Shield, onSelect: go('/srr') },
        { id: 'p-bill', group: 'pages', title: t('nav.billing'), icon: Receipt, onSelect: go('/infra-billing') },
        { id: 'p-users', group: 'pages', title: t('nav.users'), icon: Users, onSelect: go('/users') },
        { id: 'p-audit', group: 'pages', title: t('nav.audit'), icon: HardDrive, onSelect: go('/audit') },
        { id: 'p-settings', group: 'pages', title: t('nav.settings'), icon: Settings, onSelect: go('/settings') },
      );
    }

    if (isSuperAdmin) {
      pages.push(
        { id: 'p-backup', group: 'pages', title: t('nav.backup'), icon: HardDrive, onSelect: go('/backup') },
        { id: 'p-ct', group: 'pages', title: t('nav.configTransfer'), icon: FileCode2, onSelect: go('/config-transfer') },
      );
    }

    const actions: PaletteEntry[] = [
      {
        id: 'a-theme',
        group: 'actions',
        title: t('command.toggleTheme'),
        icon: Sparkles,
        onSelect: () => {
          toggleTheme();
          onOpenChange(false);
        },
      },
      {
        id: 'a-density',
        group: 'actions',
        title: t('command.toggleDensity'),
        icon: Activity,
        onSelect: () => {
          toggleDensity();
          onOpenChange(false);
        },
      },
      {
        id: 'a-sidebar',
        group: 'actions',
        title: t('command.toggleSidebar'),
        icon: Terminal,
        onSelect: () => {
          toggleSidebarCollapsed();
          onOpenChange(false);
        },
      },
      {
        id: 'a-lang',
        group: 'actions',
        title: t('command.switchLang'),
        icon: Globe,
        onSelect: () => {
          setLanguage(language === 'en' ? 'ru' : 'en');
          onOpenChange(false);
        },
      },
      {
        id: 'a-docs',
        group: 'actions',
        title: t('command.openDocs'),
        icon: FileCode2,
        onSelect: () => {
          window.open('/api/docs', '_blank', 'noopener,noreferrer');
          onOpenChange(false);
        },
      },
    ];

    return [...pages, ...actions];
  }, [
    isAdmin,
    isSuperAdmin,
    language,
    navigate,
    onOpenChange,
    setLanguage,
    t,
    toggleDensity,
    toggleSidebarCollapsed,
    toggleTheme,
  ]);

  const entries = useMemo<PaletteEntry[]>(() => {
    const q = query.trim().toLowerCase();
    const filteredBase = q
      ? baseEntries.filter((e) => e.title.toLowerCase().includes(q))
      : baseEntries;

    const dynamic: PaletteEntry[] = [
      ...clients.map((c) => ({
        id: `c-${c.id}`,
        group: 'clients' as const,
        title: c.username,
        subtitle: c.email || c.uuid,
        icon: Users,
        onSelect: () => {
          navigate(`/clients/${c.id}/traffic`);
          onOpenChange(false);
        },
      })),
      ...nodes.map((n) => ({
        id: `n-${n.id}`,
        group: 'nodes' as const,
        title: n.name,
        subtitle: `${n.host}:${n.port} (${n.status})`,
        icon: Server,
        onSelect: () => {
          navigate(`/nodes/${n.id}`);
          onOpenChange(false);
        },
      })),
      ...inbounds.map((i) => ({
        id: `i-${i.id}`,
        group: 'inbounds' as const,
        title: i.tag,
        subtitle: `${i.protocol} :${i.port}`,
        icon: Network,
        onSelect: () => {
          navigate('/inbounds');
          onOpenChange(false);
        },
      })),
    ];

    return [...filteredBase, ...dynamic];
  }, [baseEntries, clients, inbounds, navigate, nodes, onOpenChange, query]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [entries.length]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (entries.length ? (prev + 1) % entries.length : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (entries.length ? (prev - 1 + entries.length) % entries.length : 0));
    } else if (e.key === 'Enter' && entries[selectedIndex]) {
      e.preventDefault();
      entries[selectedIndex].onSelect();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="xl" className="p-0 overflow-hidden">
        <DialogHeader className="sr-only">
          <DialogTitle>Command Palette</DialogTitle>
        </DialogHeader>
        <div className="flex items-center border-b px-3 py-2.5">
          <Search className="mr-2 h-4 w-4 text-muted-foreground" />
          <Input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('command.placeholder')}
            className="border-0 shadow-none focus-visible:ring-0"
          />
        </div>
        <div className="max-h-80 overflow-y-auto p-2">
          {entries.length === 0 ? (
            <div className="py-8 text-center text-xs text-muted-foreground">{t('command.noResults')}</div>
          ) : (
            <div className="space-y-1">
              {entries.map((entry, idx) => {
                const Icon = entry.icon;
                const active = idx === selectedIndex;
                return (
                  <button
                    key={entry.id}
                    type="button"
                    onClick={entry.onSelect}
                    onMouseEnter={() => setSelectedIndex(idx)}
                    className={cn(
                      'flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm transition-colors',
                      active ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/50',
                    )}
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <Icon className="h-4 w-4 shrink-0 text-primary" />
                      <div className="truncate">
                        <span className="font-medium">{entry.title}</span>
                        {entry.subtitle && (
                          <span className="ml-2 text-xs text-muted-foreground">{entry.subtitle}</span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      <Badge variant="outline" className="text-[10px] uppercase">
                        {entry.group}
                      </Badge>
                      <ArrowRight className="h-3.5 w-3.5 text-muted-foreground" />
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <div className="flex items-center justify-between border-t bg-muted/20 px-3 py-1.5 text-[11px] text-muted-foreground">
          <span>{t('command.footer')}</span>
          <span className="font-mono">{t('command.itemsCount', { count: entries.length })}</span>
        </div>
      </DialogContent>
    </Dialog>
  );
}
