import { installWorkareaTheme } from '../views/workarea-theme';

installWorkareaTheme();

void import('../views/work').catch((error: unknown) => {
  const root = document.getElementById('root');
  if (root) root.textContent = `Could not load Work: ${error instanceof Error ? error.message : String(error)}`;
});
