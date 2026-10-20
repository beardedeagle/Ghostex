/**
 * CDXC:WorkMode 2026-10-12 WHY:
 * react-native-enriched-markdown's web build imports `Platform` and `processColor` from `react-native` (apps alias it to react-native-web) only to see that it runs on the web and pass colours through. The CEF build points that one import here (apps/desktop/vite.config.ts) instead of bundling react-native-web.
 */
export const Platform = { OS: 'web' } as const;

export function processColor(color: string): string {
  return color;
}
