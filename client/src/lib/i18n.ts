import { useCallback } from 'react';
import { translations } from '@/i18n/translations';
import { useUiStore } from '@/lib/store';

export function translate(
  language: 'en' | 'ru',
  key: string,
  vars?: Record<string, string | number>,
): string {
  const dict = translations[language] ?? translations.en;
  let value = dict[key] ?? translations.en[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      value = value.replace(new RegExp(`\\{\\{${k}\\}\\}`, 'g'), String(v));
    }
  }
  return value;
}

export function useTranslation() {
  const language = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);

  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => translate(language, key, vars),
    [language],
  );

  return { t, language, setLanguage };
}
