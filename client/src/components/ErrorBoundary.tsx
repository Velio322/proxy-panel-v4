import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('Unhandled render error:', error, info.componentStack);
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="flex min-h-screen items-center justify-center p-6">
        <div className="max-w-md space-y-4 rounded-xl border border-border/80 bg-background/60 p-6 text-center">
          <div className="text-lg font-semibold text-foreground">Что-то сломалось</div>
          <p className="text-xs text-muted-foreground">
            Ошибка интерфейса: {error.message || 'unknown'}
          </p>
          <Button onClick={() => window.location.reload()}>Перезагрузить</Button>
        </div>
      </div>
    );
  }
}
