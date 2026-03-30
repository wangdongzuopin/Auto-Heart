import { useEffect, useState } from 'react';
import { useSettings } from './useSettings';

export function useResolvedTheme() {
  const { settings } = useSettings();
  const [systemTheme, setSystemTheme] = useState<'light' | 'dark'>(() =>
    window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  );

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (event: MediaQueryListEvent) => {
      setSystemTheme(event.matches ? 'dark' : 'light');
    };

    setSystemTheme(media.matches ? 'dark' : 'light');
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, []);

  const resolvedTheme = settings.themeMode === 'system' ? systemTheme : settings.themeMode;

  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    document.documentElement.style.colorScheme = resolvedTheme;
    document.body.dataset.theme = resolvedTheme;
  }, [resolvedTheme]);

  return {
    themeMode: settings.themeMode,
    resolvedTheme,
    systemTheme,
  };
}
