/**
 * CDXC:WorkMode 2026-10-12 WHY:
 * The package's main entry is its React Native build (it imports `react-native`), so the Work page imports the web build's file directly. Its own web typings import nothing from React Native; they are re-exported here because a `.js` path has no typings beside it.
 */
declare module 'react-native-enriched-markdown/lib/module/web/EnrichedMarkdownText.js' {
  export {
    EnrichedMarkdownText,
    default,
  } from 'react-native-enriched-markdown/lib/typescript/src/web/EnrichedMarkdownText';
}
