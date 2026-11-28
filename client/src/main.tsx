import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from '@/App';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { toast } from '@/hooks/use-toast';
import { useUiStore } from '@/lib/store';
import '@/index.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      gcTime: 120_000,
      retry: 1,
      refetchOnWindowFocus: false,
      refetchIntervalInBackground: false,
    },
  },
  queryCache: new QueryCache({
    onError: (error) => {
      const status = (error as { response?: { status?: number } })?.response?.status;
      if (status === 401) return;
      const ru = useUiStore.getState().language !== 'en';
      toast({
        title: ru ? 'Ошибка загрузки данных' : 'Failed to load data',
        description: error instanceof Error ? error.message : String(error),
        variant: 'destructive',
      });
    },
  }),
  mutationCache: new MutationCache({
    onError: (error, _variables, _context, mutation) => {
      if (mutation.options.onError) return;
      const status = (error as { response?: { status?: number } })?.response?.status;
      if (status === 401) return;
      const ru = useUiStore.getState().language !== 'en';
      toast({
        title: ru ? 'Ошибка операции' : 'Action failed',
        description: error instanceof Error ? error.message : String(error),
        variant: 'destructive',
      });
    },
  }),
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ErrorBoundary>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </ErrorBoundary>
    </QueryClientProvider>
  </React.StrictMode>,
);
