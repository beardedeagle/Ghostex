/** The embed page's styles (manage.html): one HTML file, drawing or media file filling the page. */
export const MANAGE_STYLES = `
  :root {
    color-scheme: dark;
    --manage-bg: var(--app-background, light-dark(#f7f7f8, #0e0e0e));
    --manage-border-strong: light-dark(rgba(0, 0, 0, 0.12), rgba(255, 255, 255, 0.12));
    --manage-text: light-dark(#27272a, #e5e5e5);
    --manage-muted: light-dark(#626269, #a3a3a3);
    --manage-accent-muted: light-dark(rgba(0, 0, 0, 0.055), rgba(255, 255, 255, 0.055));
    --manage-red: light-dark(#be123c, #fda4af);
    background: var(--manage-bg);
  }

  * {
    box-sizing: border-box;
  }

  html,
  body,
  #root {
    background: var(--manage-bg);
    height: 100%;
    margin: 0;
    overflow: hidden;
    width: 100%;
  }

  /* The shared theme sheet gives #root the sidebar's grid layout; this page is one full-height child. */
  #root {
    display: block;
  }

  /* The native Docs view's browser area (embed.tsx): one HTML file or drawing fills the page. */
  .manage-embed {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .manage-embed > * {
    flex: 1 1 auto;
    min-height: 0;
  }

  /* A video or audio file in the Files view: the player centred, never larger than the view. */
  .manage-embed-media {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 0;
    padding: 24px;
    box-sizing: border-box;
  }

  .manage-embed-media video {
    max-width: 100%;
    max-height: 100%;
    border-radius: 8px;
    background: #000;
  }

  .manage-embed-media audio {
    width: min(560px, 100%);
  }

  /* An AVIF picture: centred and never larger than the view, like the native picture view. */
  .manage-embed-image {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  /* A PDF: Chromium's own viewer fills the view. */
  .manage-embed-pdf {
    border: 0;
    display: block;
    height: 100%;
    width: 100%;
  }

  /* A font file's preview page (preview/font-viewer.tsx): one scrolling column. */
  .manage-embed-font {
    height: 100%;
    overflow: auto;
    padding: 28px 32px 48px;
  }

  .manage-font-header {
    border-bottom: 1px solid var(--manage-border-strong);
    padding-bottom: 20px;
  }

  .manage-font-title {
    font-size: 40px;
    font-weight: normal;
    line-height: 1.15;
    margin: 0 0 10px;
    overflow-wrap: anywhere;
  }

  .manage-font-facts {
    color: var(--manage-muted);
    display: flex;
    flex-wrap: wrap;
    font-size: 12.5px;
    gap: 6px 14px;
  }

  .manage-font-credit {
    color: var(--manage-muted);
    font-size: 11.5px;
    margin-top: 8px;
    opacity: 0.8;
  }

  .manage-font-section {
    border-bottom: 1px solid var(--manage-border-strong);
    padding: 20px 0;
  }

  .manage-font-section:last-child {
    border-bottom: 0;
  }

  .manage-font-section-title {
    align-items: baseline;
    color: var(--manage-muted);
    display: flex;
    font-size: 12px;
    font-weight: 500;
    gap: 8px;
    letter-spacing: 0.04em;
    margin: 0 0 12px;
    text-transform: uppercase;
  }

  .manage-font-section-title span {
    letter-spacing: 0;
    opacity: 0.75;
    text-transform: none;
  }

  .manage-font-sample-input {
    background: var(--manage-accent-muted);
    border: 1px solid var(--manage-border-strong);
    border-radius: 7px;
    color: var(--manage-text);
    font-size: 13px;
    height: 30px;
    margin-bottom: 14px;
    outline: 0;
    padding: 0 10px;
    width: min(560px, 100%);
  }

  .manage-font-sample-input:focus {
    border-color: light-dark(rgba(0, 0, 0, 0.3), rgba(255, 255, 255, 0.3));
  }

  .manage-font-sample-row {
    align-items: baseline;
    display: flex;
    gap: 16px;
    padding: 6px 0;
  }

  .manage-font-sample-size {
    color: var(--manage-muted);
    flex: none;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    text-align: right;
    width: 28px;
  }

  .manage-font-sample-text {
    line-height: 1.25;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-font-glyph-grid {
    display: grid;
    gap: 6px;
    grid-template-columns: repeat(auto-fill, minmax(64px, 1fr));
  }

  .manage-font-glyph {
    align-items: center;
    border: 1px solid var(--manage-border-strong);
    border-radius: 7px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    justify-content: center;
    padding: 10px 4px 6px;
  }

  .manage-font-glyph-char {
    font-size: 26px;
    line-height: 1.1;
  }

  .manage-font-glyph-code {
    color: var(--manage-muted);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
  }

  body {
    color: var(--manage-text);
    font-family: "Inter Variable", Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  .manage-text-editor {
    background: var(--manage-bg);
    border: 0;
    color: light-dark(rgba(24, 24, 27, 0.88), rgba(248, 250, 252, 0.88));
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 12px;
    height: 100%;
    line-height: 1.55;
    margin: 0;
    min-height: 0;
    outline: 0;
    overflow: auto;
    padding: 16px 18px 28px;
    resize: none;
    tab-size: 2;
    white-space: pre;
    width: 100%;
  }

  /*
   * CDXC:Docs 2026-06-29-17:25:
   * Rendered HTML Docs should give the artifact an isolated browser-like viewport. Do not apply Ghostex typography, padding, link colors, or dark background to the iframe because the HTML document's own CSS must decide how the page looks.
   *
   * CDXC:Docs 2026-06-30-04:41:
   * The iframe element itself should not paint a white scrollbar gutter around dark HTML documents. Keep it transparent over the Manage background while the loaded document still owns its actual page background.
   */
  .manage-html-render-view {
    background: transparent;
    border: 0;
    color-scheme: dark;
    display: block;
    height: 100%;
    min-height: 0;
    min-width: 0;
    width: 100%;
  }

  .manage-drawing-editor {
    background: light-dark(#fafafa, #101112);
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    position: relative;
  }

  .manage-drawing-editor .excalidraw {
    min-height: 0;
  }

  .manage-drawing-error {
    align-items: center;
    background: rgba(253, 164, 175, 0.12);
    border: 1px solid rgba(253, 164, 175, 0.3);
    color: var(--manage-red);
    display: flex;
    font-size: 12px;
    gap: 7px;
    left: 12px;
    max-width: calc(100% - 24px);
    padding: 7px 9px;
    position: absolute;
    top: 12px;
    z-index: 3;
  }

  .manage-drawing-source {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
  }

  .manage-preview-message {
    align-items: center;
    color: var(--manage-muted);
    display: flex;
    gap: 10px;
    height: 100%;
    justify-content: center;
    min-height: 140px;
    padding: 24px;
  }

  .manage-preview-message span {
    font-size: 13px;
    font-weight: 500;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .manage-preview-message[data-has-action="true"] {
    flex-direction: column;
  }

  .manage-preview-message-action {
    align-items: center;
    background: transparent;
    border: 1px solid var(--manage-border-strong);
    border-radius: 7px;
    color: var(--manage-text);
    cursor: pointer;
    display: inline-flex;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
  }

  .manage-preview-message-action:hover {
    background: var(--manage-accent-muted);
  }

  @media (max-width: 760px) {
    .manage-text-editor {
      padding-left: 14px;
      padding-right: 14px;
    }
  }
`;
