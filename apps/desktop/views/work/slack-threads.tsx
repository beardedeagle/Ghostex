import { IconExternalLink, IconHash, IconPaperclip } from '@tabler/icons-react';
import { Avatar, Card } from './components';
import { relativeTime } from './format';
import { MediaPlayer, SlackText } from './rich-text';
import type { SlackMessage, SlackThread, SlackThreadRole } from './types';

const ROLE_LABEL: Record<SlackThreadRole, string> = {
  working: 'working thread',
  source: 'source thread',
  watchOnly: 'watch-only channel',
};

/**
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: the ticket shows its Slack threads' actual messages with "Open in Slack": a long thread
 * shows its first message and the latest replies, and Loom, YouTube and video links play inline.
 * The ticket's working thread is marked as such.
 */
export function SlackThreadCards({
  threads,
  now,
  onOpenUrl,
}: {
  threads: SlackThread[];
  now: number;
  onOpenUrl: (url: string) => void;
}) {
  return (
    <>
      {threads.map((thread) => (
        <SlackThreadCard key={thread.id} thread={thread} now={now} onOpenUrl={onOpenUrl} />
      ))}
    </>
  );
}

function SlackThreadCard({
  thread,
  now,
  onOpenUrl,
}: {
  thread: SlackThread;
  now: number;
  onOpenUrl: (url: string) => void;
}) {
  const [first, ...latest] = thread.messages;
  const replies = `${thread.replyCount} repl${thread.replyCount === 1 ? 'y' : 'ies'}`;
  return (
    <Card
      className={`slack-thread-card slack-thread-${thread.role}`}
      icon={<IconHash size={14} className='c-slack' />}
      title={thread.channelName ?? 'Slack'}
      sub={
        <>
          {thread.role === 'working' ? (
            <span className='w-pill is-slack working-thread-badge'>Working thread</span>
          ) : (
            `· ${ROLE_LABEL[thread.role]}`
          )}{' '}
          · {replies}
        </>
      }
      action={
        thread.permalink ? (
          <button type='button' className='w-ext open-in-slack' onClick={() => onOpenUrl(thread.permalink ?? '')}>
            <IconExternalLink size={12} />
            Open in Slack
          </button>
        ) : null
      }
    >
      <div className='w-card-body w-slack-messages'>
        {first ? (
          <SlackMessageRow message={first} now={now} onOpenUrl={onOpenUrl} />
        ) : (
          <div className='w-faint'>No messages saved yet.</div>
        )}
        {thread.hiddenReplyCount > 0 ? (
          <button
            type='button'
            className='w-link-btn w-slack-more'
            disabled={!thread.permalink}
            onClick={() => onOpenUrl(thread.permalink ?? '')}
          >
            {thread.hiddenReplyCount} earlier repl
            {thread.hiddenReplyCount === 1 ? 'y' : 'ies'} in Slack
          </button>
        ) : null}
        {latest.map((message) => (
          <SlackMessageRow key={message.ts} message={message} now={now} onOpenUrl={onOpenUrl} />
        ))}
      </div>
    </Card>
  );
}

function SlackMessageRow({
  message,
  now,
  onOpenUrl,
}: {
  message: SlackMessage;
  now: number;
  onOpenUrl: (url: string) => void;
}) {
  const author = message.authorName ?? (message.isApp ? 'Ghostex' : message.userId) ?? 'Someone';
  return (
    <div className='w-comment w-slack-message'>
      {message.isApp ? (
        <span className='w-avatar w-avatar--app' aria-hidden>
          {author.slice(0, 1)}
        </span>
      ) : (
        <Avatar name={author} size={20} />
      )}
      <div className='w-comment-main'>
        <div className='w-comment-who'>
          {author}
          <span className='w-faint' title={new Date(message.postedAt).toLocaleString()}>
            {relativeTime(new Date(message.postedAt).toISOString(), now)}
            {message.editedAt ? ' · edited' : ''}
          </span>
        </div>
        <div className='w-comment-text'>
          <SlackText text={message.text} onOpenUrl={onOpenUrl} />
        </div>
        {message.media.map((media) => (
          <div key={media.embedUrl} className='w-media w-slack-media'>
            <MediaPlayer media={media} />
          </div>
        ))}
        {message.files.map((file) => (
          <button
            key={file.id}
            type='button'
            className='w-file-row w-slack-file'
            disabled={!file.permalink}
            onClick={() => onOpenUrl(file.permalink ?? '')}
          >
            <IconPaperclip size={13} />
            <span className='w-file-name'>{file.name ?? 'File'}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
