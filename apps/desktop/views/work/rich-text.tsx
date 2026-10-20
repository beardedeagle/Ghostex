import { useState, type ReactNode } from 'react';
import { MENTION_URL_PREFIX, WorkMarkdown, clampMarkdown } from './markdown';
import type { WorkMedia } from './types';

/** The start of a long Markdown text, with Show more. */
export function ClampedText({
  text,
  limit = 700,
  onOpenUrl,
}: {
  text: string;
  limit?: number;
  onOpenUrl: (url: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const trimmed = text.trim();
  if (!trimmed) return <p className='w-faint'>No description.</p>;
  const long = trimmed.length > limit;
  const shown = long && !expanded ? clampMarkdown(trimmed, limit) : trimmed;
  return (
    <div className='w-md-clamp'>
      <WorkMarkdown markdown={shown} onOpenUrl={onOpenUrl} />
      {long ? (
        <button type='button' className='w-link-btn' onClick={() => setExpanded((value) => !value)}>
          {expanded ? 'Show less' : 'Show more'}
        </button>
      ) : null}
    </div>
  );
}

/**
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: videos and Loom or YouTube links play right on the ticket's page, which is why the Work
 * page is a web page.
 */
export function MediaPlayer({ media }: { media: WorkMedia }): ReactNode {
  if (media.kind === 'video') {
    return <video className='w-video' src={media.embedUrl} controls preload='metadata' />;
  }
  return (
    <iframe
      className='w-video'
      src={media.embedUrl}
      title={media.kind === 'loom' ? 'Loom video' : 'YouTube video'}
      allow='autoplay; fullscreen; picture-in-picture; encrypted-media'
      allowFullScreen
      referrerPolicy='strict-origin-when-cross-origin'
    />
  );
}

/** Slack's link forms (`<url|label>`, `<url>`) and bare links, then `*bold*`, `_italic_`, `` `code` `` and mentions. */
const SLACK_TOKEN =
  /<(https?:\/\/[^>|]+)\|([^>]+)>|<(https?:\/\/[^>]+)>|(https?:\/\/[^\s<>"')\]]+)|`([^`\n]+)`|\*([^*\n]+)\*|(?<![\w])_([^_\n]+)_(?![\w])|(?<![\w])(@[\w\-]+(?:\.[\w\-]+)*)/gu;

function unescapeSlack(text: string): string {
  return text.replace(/&lt;/gu, '<').replace(/&gt;/gu, '>').replace(/&amp;/gu, '&');
}

/** Slack text as Markdown text: a `<` the person typed stays a character, not an HTML tag or autolink. */
function slackPlain(text: string): string {
  return unescapeSlack(text).replace(/</gu, '\\<');
}

/**
 * Slack's mrkdwn as CommonMark, so Slack messages render through the same Markdown view as
 * tickets: `*bold*` → `**bold**`, `_italic_` → `*italic*`, `<url|label>` → `[label](url)`, and
 * mentions become `mention:` links that WorkMarkdown draws as chips and never opens. gxserver's
 * team data names mentions already (`@Rana`, `#bugs`), so only the marks are left.
 */
export function slackToMarkdown(text: string): string {
  let markdown = '';
  let last = 0;
  const link = (url: string, label: string) => `[${slackPlain(label).replace(/[[\]\\]/gu, '\\$&')}](<${url}>)`;
  for (const match of text.matchAll(SLACK_TOKEN)) {
    const at = match.index ?? 0;
    if (at > last) markdown += slackPlain(text.slice(last, at));
    const [whole, labelledUrl, label, angledUrl, bareUrl, code, bold, italic, mention] = match;
    if (labelledUrl) markdown += link(labelledUrl, label ?? labelledUrl);
    else if (angledUrl) markdown += `<${angledUrl}>`;
    else if (bareUrl) markdown += `<${bareUrl}>`;
    else if (code) markdown += `\`${unescapeSlack(code)}\``;
    else if (bold) markdown += `**${slackPlain(bold)}**`;
    else if (italic) markdown += `*${slackPlain(italic)}*`;
    else if (mention) markdown += `[${mention}](${MENTION_URL_PREFIX}${mention.slice(1)})`;
    else markdown += whole;
    last = at + match[0].length;
  }
  if (last < text.length) markdown += slackPlain(text.slice(last));
  return markdown;
}

/** A Slack message as Slack draws it: links clickable, bold, italic, code, quotes and mentions. */
export function SlackText({ text, onOpenUrl }: { text: string; onOpenUrl: (url: string) => void }) {
  return <WorkMarkdown markdown={slackToMarkdown(text)} variant='comment' onOpenUrl={onOpenUrl} />;
}
