import { create } from 'zustand';
import { startAuthentication } from '@simplewebauthn/browser';
import { applyBrandingToDocument, DEFAULT_PANEL_NAME } from '@/components/branding/BrandLogo';
import {
  authApi,
  publicConfigApi,
  registerUnauthorizedHandler,
  settingsApi,
} from '@/lib/api';
import { setFormattingDefaults } from '@/lib/core-utils';
import type { BrandingConfig, Language, PublicConfig, TelegramOAuthPayload, User } from '@/types';

const UI_STORAGE_KEY = 'mypanel3_ui_prefs';
const CURRENT_PREFS_VERSION = 2;

export type UiDensity = 'comfortable' | 'compact';

interface UiPrefs {
  prefsVersion: number;
  theme: 'dark' | 'light';
  language: Language;
  timezone: string;
  sidebarCollapsed: boolean;
  density: UiDensity;
  companyName: string;
}

function normalizeBrandName(raw: string | undefined | null): string {
  const trimmed = typeof raw === 'string' ? raw.trim() : '';
  if (!trimmed || trimmed === 'ProxyPanel' || trimmed === 'mypanel3') {
    return DEFAULT_PANEL_NAME;
  }
  return trimmed;
}

function readUiPrefs(): UiPrefs {
  const fallbackTz =
    typeof Intl !== 'undefined' ? Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC' : 'UTC';
  const defaults: UiPrefs = {
    prefsVersion: CURRENT_PREFS_VERSION,
    theme: 'dark',
    language: 'ru',
    timezone: fallbackTz,
    sidebarCollapsed: false,
    density: 'comfortable',
    companyName: DEFAULT_PANEL_NAME,
  };
  if (typeof window === 'undefined') return defaults;
  try {
    const raw = window.localStorage.getItem(UI_STORAGE_KEY);
    if (!raw) {
      writeUiPrefs(defaults);
      return defaults;
    }
    const parsed = JSON.parse(raw) as Partial<UiPrefs>;
    const needsUpgrade = (parsed.prefsVersion ?? 0) < CURRENT_PREFS_VERSION;
    const legacyName = parsed.companyName === 'ProxyPanel' || parsed.companyName === 'mypanel3';
    const resolved: UiPrefs = {
      prefsVersion: CURRENT_PREFS_VERSION,
      theme: needsUpgrade ? 'dark' : parsed.theme === 'light' ? 'light' : 'dark',
      language: needsUpgrade ? 'ru' : parsed.language === 'en' ? 'en' : 'ru',
      timezone: typeof parsed.timezone === 'string' && parsed.timezone ? parsed.timezone : fallbackTz,
      sidebarCollapsed: Boolean(parsed.sidebarCollapsed),
      density: parsed.density === 'compact' ? 'compact' : 'comfortable',
      companyName: normalizeBrandName(parsed.companyName),
    };
    if (needsUpgrade || legacyName) {
      writeUiPrefs(resolved);
    }
    return resolved;
  } catch {
    return defaults;
  }
}

function writeUiPrefs(prefs: UiPrefs) {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(UI_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // ignore storage errors
  }
}

function applyThemeToDom(theme: 'dark' | 'light') {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

function applyLanguageToDom(lang: Language) {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = lang;
}

function applyDensityToDom(density: UiDensity) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.density = density;
}

function persistFromState(s: UiPrefs) {
  writeUiPrefs({
    prefsVersion: CURRENT_PREFS_VERSION,
    theme: s.theme,
    language: s.language,
    timezone: s.timezone,
    sidebarCollapsed: s.sidebarCollapsed,
    density: s.density,
    companyName: s.companyName,
  });
}

export interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  pending2faToken: string | null;
  loadUser: () => Promise<void>;
  login: (username: string, password: string) => Promise<{ requires2fa: boolean }>;
  verify2fa: (code: string) => Promise<void>;
  clearPending2fa: () => void;
  loginWithPasskey: (username?: string) => Promise<void>;
  loginWithTelegram: (payload: TelegramOAuthPayload) => Promise<void>;
  logout: () => Promise<void>;
  clearAuth: () => void;
}

// Zero JWT or user credentials stored in localStorage. Session lives exclusively in HttpOnly cookie.
export const useAuthStore = create<AuthState>((set, get) => ({
  user: null,
  isAuthenticated: false,
  isLoading: true,
  pending2faToken: null,

  loadUser: async () => {
    set({ isLoading: true });
    try {
      const user = await authApi.me();
      set({ user, isAuthenticated: true, pending2faToken: null });
      if (user.role === 'SUPER_ADMIN') {
        settingsApi
          .get()
          .then((s) => {
            if (s.timezone) useUiStore.getState().setTimezone(s.timezone);
            if (!s.panel_branding && s.white_label_enabled === 'true' && s.company_name) {
              const clean = normalizeBrandName(s.company_name);
              if (clean !== DEFAULT_PANEL_NAME) {
                useUiStore.getState().setCompanyName(clean);
              }
            }
          })
          .catch(() => {});
      }
    } catch {
      set({ user: null, isAuthenticated: false });
    } finally {
      set({ isLoading: false });
    }
  },

  login: async (username: string, password: string) => {
    const res = await authApi.login({ username, password });
    if (res.requires2fa || res.totpRequired) {
      set({
        pending2faToken: res.challengeToken || res.tempToken || 'pending',
        isAuthenticated: false,
      });
      return { requires2fa: true };
    }
    if (res.user) {
      set({ user: res.user, isAuthenticated: true, pending2faToken: null, isLoading: false });
    } else {
      await get().loadUser();
    }
    return { requires2fa: false };
  },

  verify2fa: async (code: string) => {
    const challengeToken = get().pending2faToken;
    if (!challengeToken) {
      throw new Error('No active 2FA challenge');
    }
    const res = await authApi.verify2fa({ challengeToken, code });
    set({
      user: res.user,
      isAuthenticated: true,
      pending2faToken: null,
      isLoading: false,
    });
  },

  clearPending2fa: () => set({ pending2faToken: null }),

  loginWithPasskey: async (username?: string) => {
    const { options, challengeId } = await authApi.startPasskeyLogin(username);
    const credential = await startAuthentication({ optionsJSON: options });
    const res = await authApi.finishPasskeyLogin({ challengeId, credential });
    set({ user: res.user, isAuthenticated: true, pending2faToken: null, isLoading: false });
  },

  loginWithTelegram: async (payload: TelegramOAuthPayload) => {
    const res = await authApi.telegramLogin(payload);
    set({ user: res.user, isAuthenticated: true, pending2faToken: null, isLoading: false });
  },

  logout: async () => {
    try {
      await authApi.logout();
    } finally {
      set({ user: null, isAuthenticated: false, pending2faToken: null });
    }
  },

  clearAuth: () => {
    set({ user: null, isAuthenticated: false, pending2faToken: null, isLoading: false });
  },
}));

registerUnauthorizedHandler(() => {
  useAuthStore.getState().clearAuth();
});

export interface PublicConfigState {
  config: PublicConfig | null;
  branding: BrandingConfig;
  healthVersion: string | null;
  healthStatus: 'ok' | 'degraded' | 'unknown';
  loaded: boolean;
  setBranding: (branding: BrandingConfig) => void;
  fetchPublicConfig: () => Promise<void>;
}

export interface UiState extends UiPrefs {
  setTheme: (theme: 'dark' | 'light') => void;
  toggleTheme: () => void;
  setLanguage: (lang: Language) => void;
  setTimezone: (tz: string) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebarCollapsed: () => void;
  setDensity: (density: UiDensity) => void;
  toggleDensity: () => void;
  setCompanyName: (name: string) => void;
}

const initialPrefs = readUiPrefs();
applyThemeToDom(initialPrefs.theme);
applyLanguageToDom(initialPrefs.language);
applyDensityToDom(initialPrefs.density);
setFormattingDefaults({ timezone: initialPrefs.timezone, language: initialPrefs.language });

export const useUiStore = create<UiState>((set, get) => ({
  ...initialPrefs,
  setTheme: (theme) => {
    applyThemeToDom(theme);
    const next = { ...get(), theme };
    persistFromState(next);
    set({ theme });
  },
  toggleTheme: () => {
    const nextTheme = get().theme === 'dark' ? 'light' : 'dark';
    get().setTheme(nextTheme);
  },
  setLanguage: (language) => {
    const safeLang: Language = language === 'en' ? 'en' : 'ru';
    applyLanguageToDom(safeLang);
    setFormattingDefaults({ language: safeLang });
    const next = { ...get(), language: safeLang };
    persistFromState(next);
    set({ language: safeLang });
  },
  setTimezone: (timezone) => {
    const safeTz = timezone.trim() || 'UTC';
    setFormattingDefaults({ timezone: safeTz });
    const next = { ...get(), timezone: safeTz };
    persistFromState(next);
    set({ timezone: safeTz });
  },
  setSidebarCollapsed: (sidebarCollapsed) => {
    const next = { ...get(), sidebarCollapsed };
    persistFromState(next);
    set({ sidebarCollapsed });
  },
  toggleSidebarCollapsed: () => {
    get().setSidebarCollapsed(!get().sidebarCollapsed);
  },
  setDensity: (density) => {
    const safeDensity: UiDensity = density === 'compact' ? 'compact' : 'comfortable';
    applyDensityToDom(safeDensity);
    const next = { ...get(), density: safeDensity };
    persistFromState(next);
    set({ density: safeDensity });
  },
  toggleDensity: () => {
    const nextDensity: UiDensity = get().density === 'compact' ? 'comfortable' : 'compact';
    get().setDensity(nextDensity);
  },
  setCompanyName: (companyName) => {
    const safeName = normalizeBrandName(companyName);
    const next = { ...get(), companyName: safeName };
    persistFromState(next);
    set({ companyName: safeName });
  },
}));

export const usePublicConfigStore = create<PublicConfigState>((set) => ({
  config: null,
  branding: {
    panelName: initialPrefs.companyName || DEFAULT_PANEL_NAME,
    hasCustomLogo: false,
    logoMime: null,
    logoVersion: null,
    logoUrl: null,
  },
  healthVersion: null,
  healthStatus: 'unknown',
  loaded: false,
  setBranding: (branding) => {
    const safeName = normalizeBrandName(branding.panelName);
    const resolved: BrandingConfig = {
      ...branding,
      panelName: safeName,
      logoUrl: branding.hasCustomLogo ? branding.logoUrl ?? '/api/v1/branding/logo' : null,
    };
    useUiStore.getState().setCompanyName(safeName);
    applyBrandingToDocument(resolved.panelName, resolved.logoUrl, resolved.logoMime);
    set((prev) => ({
      branding: resolved,
      config: prev.config ? { ...prev.config, branding: resolved } : prev.config,
    }));
  },
  fetchPublicConfig: async () => {
    const [cfgRes, healthRes] = await Promise.allSettled([
      publicConfigApi.get(),
      publicConfigApi.health(),
    ]);
    const config = cfgRes.status === 'fulfilled' ? cfgRes.value : null;
    const healthVersion =
      healthRes.status === 'fulfilled'
        ? healthRes.value.version || config?.version || null
        : config?.version || null;
    const healthStatus =
      healthRes.status === 'fulfilled'
        ? healthRes.value.status === 'ok'
          ? 'ok'
          : 'degraded'
        : 'degraded';
    set({ config, healthVersion, healthStatus, loaded: true });
    if (config?.branding) {
      usePublicConfigStore.getState().setBranding(config.branding);
    } else {
      applyBrandingToDocument(useUiStore.getState().companyName || DEFAULT_PANEL_NAME, null, null);
    }
  },
}));
