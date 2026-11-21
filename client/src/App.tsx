import { Suspense, lazy, useEffect } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';
import { MainLayout } from '@/components/layout/MainLayout';
import { Toaster } from '@/components/ui/toaster';
import { useAuthStore, usePublicConfigStore } from '@/lib/store';

const LoginPage = lazy(() => import('@/pages/LoginPage').then((m) => ({ default: m.LoginPage })));
const DashboardPage = lazy(() => import('@/pages/DashboardPage').then((m) => ({ default: m.DashboardPage })));
const NodesPage = lazy(() => import('@/pages/nodes/NodesPage').then((m) => ({ default: m.NodesPage })));
const NodeDetailPage = lazy(() =>
  import('@/pages/nodes/NodeDetailPage').then((m) => ({ default: m.NodeDetailPage })),
);
const NodeLogsPage = lazy(() => import('@/pages/NodeLogsPage').then((m) => ({ default: m.NodeLogsPage })));
const InboundsPage = lazy(() => import('@/pages/inbounds/InboundsPage').then((m) => ({ default: m.InboundsPage })));
const HostsPage = lazy(() => import('@/pages/HostsPage').then((m) => ({ default: m.HostsPage })));
const ClientsPage = lazy(() => import('@/pages/clients/ClientsPage').then((m) => ({ default: m.ClientsPage })));
const ClientTrafficPage = lazy(() =>
  import('@/pages/clients/ClientTrafficPage').then((m) => ({ default: m.ClientTrafficPage })),
);
const RoutingPage = lazy(() => import('@/pages/RoutingPage').then((m) => ({ default: m.RoutingPage })));
const MonitoringPage = lazy(() => import('@/pages/MonitoringPage').then((m) => ({ default: m.MonitoringPage })));
const SessionsExplorerPage = lazy(() =>
  import('@/pages/SessionsExplorerPage').then((m) => ({ default: m.SessionsExplorerPage })),
);
const InspectorsPage = lazy(() => import('@/pages/InspectorsPage').then((m) => ({ default: m.InspectorsPage })));
const PlansPage = lazy(() => import('@/pages/PlansPage').then((m) => ({ default: m.PlansPage })));
const ResellersPage = lazy(() => import('@/pages/ResellersPage').then((m) => ({ default: m.ResellersPage })));
const SubscriptionTemplatesPage = lazy(() =>
  import('@/pages/SubscriptionTemplatesPage').then((m) => ({ default: m.SubscriptionTemplatesPage })),
);
const SrrRulesPage = lazy(() => import('@/pages/SrrRulesPage').then((m) => ({ default: m.SrrRulesPage })));
const InfraBillingPage = lazy(() => import('@/pages/InfraBillingPage').then((m) => ({ default: m.InfraBillingPage })));
const UsersPage = lazy(() => import('@/pages/UsersPage').then((m) => ({ default: m.UsersPage })));
const AuditPage = lazy(() => import('@/pages/AuditPage').then((m) => ({ default: m.AuditPage })));
const BackupPage = lazy(() => import('@/pages/BackupPage').then((m) => ({ default: m.BackupPage })));
const ConfigTransferPage = lazy(() =>
  import('@/pages/ConfigTransferPage').then((m) => ({ default: m.ConfigTransferPage })),
);
const SettingsPage = lazy(() => import('@/pages/SettingsPage').then((m) => ({ default: m.SettingsPage })));

function PageLoader() {
  return (
    <div className="flex min-h-[260px] items-center justify-center p-6 text-xs text-muted-foreground">
      <div className="flex items-center gap-2">
        <span className="h-2 w-2 animate-pulse rounded-full bg-primary" />
        <span>Loading module...</span>
      </div>
    </div>
  );
}

function ProtectedLayout() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoading = useAuthStore((s) => s.isLoading);

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background text-sm text-muted-foreground">
        Loading session...
      </div>
    );
  }

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  return <MainLayout />;
}

export function App() {
  const loadUser = useAuthStore((s) => s.loadUser);
  const fetchPublicConfig = usePublicConfigStore((s) => s.fetchPublicConfig);

  useEffect(() => {
    fetchPublicConfig();
    loadUser();
  }, [fetchPublicConfig, loadUser]);

  return (
    <>
      <Suspense fallback={<PageLoader />}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route element={<ProtectedLayout />}>
            <Route index element={<DashboardPage />} />
            <Route path="/nodes" element={<NodesPage />} />
            <Route path="/nodes/:id" element={<NodeDetailPage />} />
            <Route path="/nodes/:id/logs" element={<NodeLogsPage />} />
            <Route path="/inbounds" element={<InboundsPage />} />
            <Route path="/hosts" element={<HostsPage />} />
            <Route path="/clients" element={<ClientsPage />} />
            <Route path="/clients/:id/traffic" element={<ClientTrafficPage />} />
            <Route path="/routing" element={<RoutingPage />} />
            <Route path="/monitoring" element={<MonitoringPage />} />
            <Route path="/sessions" element={<SessionsExplorerPage />} />
            <Route path="/logs" element={<NodeLogsPage />} />
            <Route path="/inspectors" element={<InspectorsPage />} />
            <Route path="/srh-inspector" element={<InspectorsPage />} />
            <Route path="/hwid-inspector" element={<InspectorsPage />} />
            <Route path="/torrent-blocker" element={<InspectorsPage />} />
            <Route path="/plans" element={<PlansPage />} />
            <Route path="/resellers" element={<ResellersPage />} />
            <Route path="/templates" element={<SubscriptionTemplatesPage />} />
            <Route path="/srr" element={<SrrRulesPage />} />
            <Route path="/infra-billing" element={<InfraBillingPage />} />
            <Route path="/users" element={<UsersPage />} />
            <Route path="/audit" element={<AuditPage />} />
            <Route path="/backup" element={<BackupPage />} />
            <Route path="/config-transfer" element={<ConfigTransferPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </Suspense>
      <Toaster />
    </>
  );
}

export default App;
