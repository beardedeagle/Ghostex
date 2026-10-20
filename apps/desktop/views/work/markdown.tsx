import { useCallback } from 'react';
import { EnrichedMarkdownText } from 'react-native-enriched-markdown/lib/module/web/EnrichedMarkdownText.js';
import type { MarkdownStyle, Md4cFlags } from 'react-native-enriched-markdown/lib/typescript/src/types/MarkdownStyle';

/** Links the page hands to the app; anything else (`javascript:`, relative paths, mention links) does nothing. */
const OPENABLE_URL = /^(https?:|mailto:)/iu;

/** The link form `slackToMarkdown` gives Slack mentions, drawn as a mention chip. */
export const MENTION_URL_PREFIX = 'mention:';

/**
 * Line breaks show where they were typed (Linear, GitHub comments and Slack all draw them), and
 * `$` stays text: tickets talk about prices far more often than maths.
 */
const MD4C_FLAGS: Md4cFlags = {
  hardSoftBreaks: true,
  latexMath: false,
  underline: false,
};

const TEXT_FONT = 'inherit';
const MONO_FONT = 'var(--w-mono)';

function workMarkdownStyle(fontSize: number, lineHeight: number, color: string): MarkdownStyle {
  const heading = (size: number, height: number) => ({
    fontSize: size,
    lineHeight: height,
    fontFamily: TEXT_FONT,
    fontWeight: '600',
    color: 'var(--w-text)',
    marginTop: 10,
    marginBottom: 4,
  });
  return {
    paragraph: {
      fontSize,
      lineHeight,
      fontFamily: TEXT_FONT,
      color,
      marginTop: 0,
      marginBottom: 8,
    },
    h1: heading(fontSize + 3, lineHeight + 4),
    h2: heading(fontSize + 2, lineHeight + 3),
    h3: heading(fontSize + 1, lineHeight + 2),
    h4: heading(fontSize, lineHeight),
    h5: heading(fontSize, lineHeight),
    h6: heading(fontSize, lineHeight),
    list: {
      fontSize,
      lineHeight,
      fontFamily: TEXT_FONT,
      color,
      marginTop: 0,
      marginBottom: 8,
      marginLeft: 16,
      gapWidth: 8,
      bulletColor: 'var(--w-text-3)',
      bulletSize: 4,
      markerColor: 'var(--w-text-3)',
      markerFontWeight: '500',
      itemSpacing: 2,
    },
    blockquote: {
      fontSize,
      lineHeight,
      fontFamily: TEXT_FONT,
      color: 'var(--w-text-2)',
      marginTop: 0,
      marginBottom: 8,
      borderColor: 'var(--w-line-strong)',
      borderWidth: 2,
      gapWidth: 10,
      backgroundColor: 'transparent',
    },
    codeBlock: {
      fontSize: 11.5,
      lineHeight: 17,
      fontFamily: MONO_FONT,
      color: 'var(--w-text)',
      marginTop: 0,
      marginBottom: 8,
      backgroundColor: 'var(--w-muted-bg)',
      borderColor: 'var(--w-line)',
      borderRadius: 6,
      borderWidth: 1,
      padding: 8,
    },
    code: {
      fontFamily: MONO_FONT,
      fontSize: 11.5,
      color: 'var(--w-text)',
      backgroundColor: 'var(--w-muted-bg)',
      borderColor: 'transparent',
    },
    link: {
      color: 'var(--w-accent)',
      underline: false,
      backgroundColor: 'transparent',
    },
    linkVariants: {
      [`^${MENTION_URL_PREFIX}`]: {
        color: 'var(--w-accent)',
        underline: false,
        backgroundColor: 'rgba(134, 211, 248, 0.1)',
      },
    },
    strong: { fontWeight: 'bold', color: 'var(--w-text)' },
    strikethrough: { color: 'var(--w-text-3)' },
    thematicBreak: {
      color: 'var(--w-line-strong)',
      height: 1,
      marginTop: 10,
      marginBottom: 10,
    },
    table: {
      fontSize: fontSize - 1,
      lineHeight: lineHeight - 2,
      fontFamily: TEXT_FONT,
      color,
      marginTop: 0,
      marginBottom: 8,
      headerBackgroundColor: 'var(--w-muted-bg)',
      headerTextColor: 'var(--w-text)',
      rowEvenBackgroundColor: 'transparent',
      rowOddBackgroundColor: 'transparent',
      borderColor: 'var(--w-line-strong)',
      borderWidth: 1,
      borderRadius: 6,
      cellPaddingHorizontal: 8,
      cellPaddingVertical: 4,
    },
    taskList: {
      checkedColor: 'var(--w-accent)',
      borderColor: 'var(--w-text-3)',
      checkmarkColor: 'var(--w-page)',
      checkedTextColor: 'var(--w-text-3)',
      checkboxSize: 12,
      checkboxBorderRadius: 3,
    },
    image: { maxHeight: 280, borderRadius: 6, marginTop: 0, marginBottom: 8 },
  };
}

/** Descriptions and PR bodies: the page's 13px text. */
const BODY_STYLE = workMarkdownStyle(13, 20, 'var(--w-text)');
/** Comments and Slack messages: the quieter 12.5px comment text. */
const COMMENT_STYLE = workMarkdownStyle(12.5, 19, 'var(--w-text-2)');

/**
 * CDXC:WorkMode 2026-10-12 DECISION:
 * User: "make this show markdown in the work side panel / use Enriched Markdown to render the markdown here and everywhere in work side panel please". Linear and GitHub descriptions and comments, PR bodies and Slack messages are drawn by Software Mansion's react-native-enriched-markdown (its web build: plain React DOM, md4c parser as WebAssembly inlined in the bundle), and its links open through the page's `onOpenUrl` bridge.
 */
export function WorkMarkdown({
  markdown,
  variant = 'body',
  onOpenUrl,
}: {
  markdown: string;
  variant?: 'body' | 'comment';
  onOpenUrl: (url: string) => void;
}) {
  const openUrl = useCallback(
    ({ url }: { url: string }) => {
      if (OPENABLE_URL.test(url)) onOpenUrl(url);
    },
    [onOpenUrl]
  );
  return (
    <div className='w-md'>
      <EnrichedMarkdownText
        markdown={markdown}
        markdownStyle={variant === 'comment' ? COMMENT_STYLE : BODY_STYLE}
        md4cFlags={MD4C_FLAGS}
        onLinkPress={openUrl}
        onImagePress={openUrl}
        enableTaskListItemToggle={false}
      />
    </div>
  );
}

/**
 * The start of a long Markdown text for a clamped view: cut at a line end near the limit so a
 * table row or list item is never split, and close a code fence the cut left open.
 */
export function clampMarkdown(text: string, limit: number): string {
  let cut = text.slice(0, limit);
  const lineEnd = cut.lastIndexOf('\n');
  cut = lineEnd > limit / 2 ? cut.slice(0, lineEnd) : `${cut.trimEnd()}…`;
  let openFence: string | null = null;
  for (const line of cut.split('\n')) {
    const fence = /^\s{0,3}(`{3,}|~{3,})/u.exec(line)?.[1];
    if (!fence) continue;
    if (openFence === null) openFence = fence;
    else if (fence[0] === openFence[0] && fence.length >= openFence.length) openFence = null;
  }
  return openFence === null ? cut.trimEnd() : `${cut.trimEnd()}\n${openFence}`;
}
