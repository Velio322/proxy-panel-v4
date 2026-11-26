import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { browserSupportsWebAuthn } from '@simplewebauthn/browser';
import { Eye, EyeOff, KeyRound, Lock, Moon, Sun, User as UserIcon } from 'lucide-react';
import { BrandMark, DEFAULT_PANEL_NAME } from '@/components/branding/BrandLogo';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { getApiErrorMessage } from '@/lib/api';
import { useTranslation } from '@/lib/i18n';
import { useAuthStore, usePublicConfigStore, useUiStore } from '@/lib/store';
import type { TelegramOAuthPayload } from '@/types';

declare global {
  interface Window {
    onTelegramAuth?: (user: TelegramOAuthPayload) => void;
  }
}

export function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const login = useAuthStore((s) => s.login);
  const verify2fa = useAuthStore((s) => s.verify2fa);
  const clearPending2fa = useAuthStore((s) => s.clearPending2fa);
  const loginWithPasskey = useAuthStore((s) => s.loginWithPasskey);
  const loginWithTelegram = useAuthStore((s) => s.loginWithTelegram);
  const pending2faToken = useAuthStore((s) => s.pending2faToken);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);

  const publicConfig = usePublicConfigStore((s) => s.config);
  const branding = usePublicConfigStore((s) => s.branding);
  const healthVersion = usePublicConfigStore((s) => s.healthVersion);
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const language = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [totpCode, setTotpCode] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const tgContainerRef = useRef<HTMLDivElement | null>(null);
  const supportsPasskey = typeof window !== 'undefined' && browserSupportsWebAuthn();

  const displayName = branding.panelName || DEFAULT_PANEL_NAME;
  const versionBadge = healthVersion ? `v${healthVersion.replace(/^v/i, '')}` : 'v4.0.0';

  const returnTo = (): string => {
    let target = '/';
    try {
      const saved = window.sessionStorage.getItem('auth_return_to');
      if (saved?.startsWith('/') && !saved.startsWith('//')) {
        target = saved;
      }
      window.sessionStorage.removeItem('auth_return_to');
    } catch {
      // Session storage unavailable — default to home.
    }
    return target;
  };

  useEffect(() => {
    if (isAuthenticated) {
      navigate(returnTo(), { replace: true });
    }
  }, [isAuthenticated, navigate]);

  useEffect(() => {
    const bot = publicConfig?.telegramBotUsername?.trim();
    const container = tgContainerRef.current;
    if (!bot || !container || pending2faToken) return;

    window.onTelegramAuth = async (tgUser: TelegramOAuthPayload) => {
      setError(null);
      setSubmitting(true);
      try {
        await loginWithTelegram(tgUser);
        navigate(returnTo(), { replace: true });
      } catch (err) {
        setError(getApiErrorMessage(err, 'Telegram login failed'));
      } finally {
        setSubmitting(false);
      }
    };

    const script = document.createElement('script');
    script.src = 'https://telegram.org/js/telegram-widget.js?22';
    script.async = true;
    script.setAttribute('data-telegram-login', bot);
    script.setAttribute('data-size', 'large');
    script.setAttribute('data-onauth', 'onTelegramAuth(user)');
    script.setAttribute('data-request-access', 'write');
    container.innerHTML = '';
    container.appendChild(script);

    return () => {
      delete window.onTelegramAuth;
      if (container) container.innerHTML = '';
    };
  }, [publicConfig?.telegramBotUsername, pending2faToken, loginWithTelegram, navigate]);

  const handleCredentialsSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const res = await login(username.trim(), password);
      if (!res.requires2fa) {
        navigate(returnTo(), { replace: true });
      }
    } catch (err) {
      setError(getApiErrorMessage(err, t('auth.invalidCredentials')));
    } finally {
      setSubmitting(false);
    }
  };

  const handle2faSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      await verify2fa(totpCode.trim());
      navigate(returnTo(), { replace: true });
    } catch (err) {
      setError(getApiErrorMessage(err, 'Invalid 2FA code'));
    } finally {
      setSubmitting(false);
    }
  };

  const handlePasskey = async () => {
    setError(null);
    setSubmitting(true);
    try {
      await loginWithPasskey(username.trim() || undefined);
      navigate(returnTo(), { replace: true });
    } catch (err) {
      setError(getApiErrorMessage(err, 'Passkey authentication failed'));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="login-grid-bg relative flex min-h-screen flex-col items-center justify-center bg-background p-4">
      <div className="absolute right-4 top-4 flex items-center gap-1.5">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => setLanguage(language === 'en' ? 'ru' : 'en')}
          className="px-2 text-xs font-semibold uppercase"
          aria-label="Switch language"
        >
          {language}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          onClick={toggleTheme}
          aria-label="Toggle theme"
        >
          {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
      </div>

      <Card className="w-full max-w-md border-border/90 bg-card/95 shadow-2xl backdrop-blur-md">
        <CardHeader className="space-y-2 text-center">
          <div className="mx-auto mb-1 flex justify-center">
            <BrandMark logoUrl={branding.hasCustomLogo ? branding.logoUrl : null} panelName={displayName} size="xl" />
          </div>
          <div className="flex items-center justify-center gap-2">
            <CardTitle className="text-xl tracking-tight">{displayName}</CardTitle>
            <span className="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 font-mono text-[10px] font-medium text-emerald-500 dark:text-emerald-300">
              {versionBadge}
            </span>
          </div>
          <CardDescription>
            {pending2faToken ? t('auth.twoFactorHint') : t('auth.signInSubtitle')}
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-4">
          {error && (
            <div role="alert" className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-400">
              {error}
            </div>
          )}

          {pending2faToken ? (
            <form onSubmit={handle2faSubmit} className="space-y-4">
              <div className="space-y-1.5">
                <Label htmlFor="totp-code">{t('auth.twoFactorTitle')}</Label>
                <Input
                  id="totp-code"
                  type="text"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  maxLength={6}
                  placeholder="000000"
                  value={totpCode}
                  onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
                  required
                />
              </div>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="outline"
                  className="flex-1"
                  onClick={() => {
                    clearPending2fa();
                    setTotpCode('');
                    setError(null);
                  }}
                >
                  {t('auth.back')}
                </Button>
                <Button type="submit" className="flex-1" loading={submitting}>
                  {t('auth.verify')}
                </Button>
              </div>
            </form>
          ) : (
            <>
              <form onSubmit={handleCredentialsSubmit} className="space-y-4">
                <div className="space-y-1.5">
                  <Label htmlFor="login-username">{t('auth.username')}</Label>
                  <div className="relative">
                    <UserIcon className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                      id="login-username"
                      type="text"
                      autoComplete="username"
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                      className="pl-9"
                      required
                    />
                  </div>
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="login-password">{t('auth.password')}</Label>
                  <div className="relative">
                    <Lock className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                      id="login-password"
                      type={showPassword ? 'text' : 'password'}
                      autoComplete="current-password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      className="pl-9 pr-9"
                      required
                    />
                    <button
                      type="button"
                      onClick={() => setShowPassword((prev) => !prev)}
                      aria-label={showPassword ? 'Hide password' : 'Show password'}
                      className="absolute right-2.5 top-2 rounded p-0.5 text-muted-foreground hover:text-foreground"
                    >
                      {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </div>
                </div>
                <Button type="submit" className="w-full" loading={submitting}>
                  {t('auth.signIn')}
                </Button>
              </form>

              {supportsPasskey && (
                <Button
                  type="button"
                  variant="outline"
                  className="w-full"
                  onClick={handlePasskey}
                  disabled={submitting}
                >
                  <KeyRound className="h-4 w-4" />
                  {t('auth.passkey')}
                </Button>
              )}

              {publicConfig?.telegramBotUsername && (
                <div className="flex flex-col items-center gap-2 border-t pt-2">
                  <span className="text-xs text-muted-foreground">
                    {language === 'ru' ? 'Или войти через Telegram' : 'Or continue with Telegram'}
                  </span>
                  <div ref={tgContainerRef} data-testid="telegram-login-widget" />
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      <div className="mt-5 font-mono text-[11px] text-muted-foreground/75">
        {displayName} · Infrastructure Control · {versionBadge}
      </div>
    </div>
  );
}
