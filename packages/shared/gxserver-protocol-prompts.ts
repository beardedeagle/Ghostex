import type { GxserverSessionChatDraftListEntry } from './gxserver-protocol-sessions';

/*
CDXC:ServerDaemon 2026-06-24-13:30:
Pinned Prompts use shared React hydrate fields on every client, but persistence
belongs to gxserver instead of platform-local storage. These RPC payloads can
include user-authored prompt bodies, so clients must not log request params or
daemon response bodies.
*/
export interface GxserverPinnedPrompt {
  content: string;
  createdAt: string;
  promptId: string;
  title: string;
  updatedAt: string;
}

export interface GxserverAppUserData {
  pinnedPrompts: readonly GxserverPinnedPrompt[];
}

export interface GxserverSavePinnedPromptParams {
  content: string;
  promptId?: string;
  title: string;
}

/*
CDXC:SavedPrompts 2026-07-29-00:00:
Stashed prompts are captured server-side on every prompt-editor save-and-close
and recalled from the session "Prompts" modal. projectId/sessionId/cwd are
soft references describing where the prompt was composed; a stash outlives the
project or session it came from. These payloads carry user-authored prompt
bodies, so clients must not log request params or daemon response bodies.
*/
export interface GxserverStashedPrompt {
  content: string;
  createdAt: string;
  cwd: string | null;
  /**
   * Tags filed on this prompt, in rail order (built-ins first). Always present
   * on rows read from gxserver; a prompt with no tags carries an empty array.
   */
  tagIds?: readonly string[];
  /** Origin project's identity icon, shaped for `WorkspaceProjectIconSource`. */
  projectIcon?: unknown;
  projectIconDataUrl?: string | null;
  /** Repository icon discovered by gxserver, matching the active sidebar project icon. */
  projectDiscoveredIconDataUrl?: string | null;
  /**
   * RAW gxserver ids, never the sidebar's combined `combined-session:` key:
   * gxserver normalizes whatever a writer stored as of migration 0026. On a
   * list result these name the session that currently owns the prompt's
   * conversation, which can differ from the session it was stashed from once
   * that one has been resumed or forked.
   */
  projectId: string | null;
  projectName: string | null;
  promptId: string;
  sessionId: string | null;
  /**
   * The agent conversation this prompt was stashed from (migration 0026),
   * absent when there is none to resolve. It follows Claude/Codex
   * compaction-resume rewrites, so it stays valid after the provider mints a
   * successor conversation id.
   */
  agentSessionId?: string;
  /** Current title of the session that owns `agentSessionId`, when resolvable. */
  sessionTitle?: string;
  updatedAt: string;
}

export interface GxserverSaveStashedPromptParams {
  content: string;
  cwd?: string;
  /** When present, updates this saved prompt in place. */
  promptId?: string;
  projectId?: string;
  sessionId?: string;
  /**
   * Explicit filing for a manually saved prompt. Omit this only for a real
   * stash action, which assigns the builtin Stashed tag; an empty array means
   * the user deliberately chose No tag.
   */
  tagIds?: readonly string[];
}

export interface GxserverSaveStashedPromptResult {
  /** True only when this save inserted a new stash row. */
  created: boolean;
  prompt: GxserverStashedPrompt;
}

export interface GxserverListStashedPromptsParams {
  /** Omit recovery/sent history when only the saved library is needed. Defaults to true for older clients. */
  includeRecovery?: boolean;
  includeDelivered?: boolean;
  /** When present, results are limited to this project plus its worktree family. */
  projectId?: string;
}

export interface GxserverListStashedPromptsResult {
  drafts?: GxserverSessionChatDraftListEntry[];
  deliveredDrafts?: import('./session-chat-queue').SessionChatDeliveredDraft[];
  recoveryDrafts?: import('./session-chat-queue').SessionChatRecoveryDraft[];
  prompts: readonly GxserverStashedPrompt[];
  /** The tag catalogue, so the modal paints its rail and its rows together. */
  tags?: readonly GxserverStashedPromptTag[];
}

/*
CDXC:SavedPrompts 2026-08-23:
Saved Prompts are filed under daemon-owned tags. Favorites is not a separate
flag but a seeded builtin tag, so the star control and a user tag write the same
link table. Colors are stored as literal `#rrggbb` because every client
interpolates them into CSS.
*/
export interface GxserverStashedPromptTag {
  color: string;
  createdAt: string;
  /** Builtin tags have stable app-owned behavior and cannot be deleted. */
  isBuiltin: boolean;
  name: string;
  tagId: string;
  updatedAt: string;
}

/** The tagId of the seeded builtin Favorites tag. */
export const GXSERVER_FAVORITE_PROMPT_TAG_ID = 'favorite';
/** The tagId automatically assigned whenever a prompt is stashed. */
export const GXSERVER_STASHED_PROMPT_TAG_ID = 'stashed';

export interface GxserverListStashedPromptTagsResult {
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverSaveStashedPromptTagParams {
  color?: string;
  name: string;
  /** When present, renames or recolors this tag instead of creating one. */
  tagId?: string;
}

export interface GxserverSaveStashedPromptTagResult {
  tag: GxserverStashedPromptTag;
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverDeleteStashedPromptTagParams {
  tagId: string;
}

export interface GxserverDeleteStashedPromptTagResult {
  deleted: boolean;
  tags: readonly GxserverStashedPromptTag[];
}

export interface GxserverSetStashedPromptTagsParams {
  promptId: string;
  tagIds: readonly string[];
}

export interface GxserverSetStashedPromptTagsResult {
  prompt: GxserverStashedPrompt;
}

export interface GxserverDeleteStashedPromptParams {
  promptId: string;
}

export interface GxserverDeleteStashedPromptResult {
  deleted: boolean;
}

/*
CDXC:SessionNotes 2026-08-24:
A session note is keyed by the PROVIDER conversation id (`agentSessionId`), not
by the ghostex session id, so "what to do next here" survives closing the
ghostex session and resuming the same agent conversation later. Clients address
the note by (projectId, sessionId) and gxserver resolves the agent session id
itself; a session that has no provider conversation yet cannot hold a note.
These payloads carry user-authored note bodies, so clients must not log request
params or daemon response bodies.
*/
export interface GxserverSaveSessionAgentNoteParams {
  /** Trimmed server-side; an empty note deletes the stored note. */
  note: string;
  projectId: string;
  sessionId: string;
}

export interface GxserverSaveSessionAgentNoteResult {
  /** The provider conversation id the note was filed under. */
  agentSessionId: string;
  /** The stored note after trimming; empty string when the note was cleared. */
  note: string;
  /** Canonical session-row project id the note was saved through. */
  projectId?: string;
  /** Canonical session-row session id the note was saved through. */
  sessionId?: string;
}

export interface GxserverReadSessionAgentNoteParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverReadSessionAgentNoteResult {
  /** Absent when the session has no provider conversation id yet. */
  agentSessionId?: string;
  /** Absent when no note is stored; never an empty string. */
  note?: string;
}
