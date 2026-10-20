import type { AccountSwitchProgress } from './agent-accounts';
import type { SessionChatDraft, SessionChatQueuedPrompt } from './session-chat-queue';
import type {
  SessionChatMessage,
  SessionChatTurnLifecycle,
  SessionChatStatus,
  SessionChatInteractivePrompt,
} from './session-chat-transcript';
import type {
  SessionChatDetectedOptions,
  SessionChatTerminalNotice,
  SessionChatTerminalActivity,
  SessionChatReturnedPrompt,
  SessionChatAppCommand,
  SessionChatAgentFleet,
  SessionChatAgentTasks,
} from './session-chat-agent-state';
import type { SessionChatPendingModelSelection } from './session-chat-agents';
import type { SessionChatAvailableAgent } from './session-chat-rpc';

interface SessionChatFrameBase {
  /** Provider family on authoritative snapshots and replacements. */
  agent?: string;
  projectId: string;
  sessionId: string;
  /** Follower generation; bumps on start/replace/re-resolve. */
  epoch: number;
  /** Monotonic within an epoch, starting at 1. */
  seq: number;
  protocolVersion: number;
  serverId: string;
  /**
   * The session's live agent-hook activity at frame time (true = working).
   * Carried by snapshot/replaced/state frames; omitted on appended frames,
   * which never change it.
   */
  working?: boolean;
}

export interface GxserverSessionChatSnapshotEvent extends SessionChatFrameBase {
  /** Codex process start in epoch ms; earlier async questions expired on resume. Omitted means unchanged. */
  asyncQuestionsSince?: number | null;
  /** Server-confirmed answers/skips, including answers still queued inside Codex. Omitted means unchanged. */
  retiredAsyncQuestionIds?: string[];
  type: 'sessionChatSnapshot';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /**
   * A prompt Claude Code handed back to its composer after an Escape, for the
   * client to put back into its own composer. Applied once per `id` by the
   * client; omitted ⇒ nothing new (never "cleared"). Carried while fresh only.
   */
  returnedPrompt?: SessionChatReturnedPrompt;
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /** Claude's task list from its on-disk store. Omitted ⇒ cleared. */
  agentTasks?: SessionChatAgentTasks;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  accountSwitch?: AccountSwitchProgress | null;
  /** Ghostex's automatic retry after a connection or server error (server/src/accounts/api_errors.rs `chat_state`), or null. */
  apiRetry?: Record<string, unknown> | null;
  pendingModelSelection?: SessionChatPendingModelSelection | null;
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  /**
   * Mobile's SSH transport synthesizes this frame from a read and carries the
   * read-only draft-agent state as own-properties. Daemon event frames omit
   * both fields, so clients must fold them only when either property exists.
   */
  availableAgents?: SessionChatAvailableAgent[];
  switchableAgents?: SessionChatAvailableAgent[];
  sessionAgentId?: string;
  agentSessionId?: string;
}

export interface GxserverSessionChatAppendedEvent extends SessionChatFrameBase {
  type: 'sessionChatAppended';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  /**
   * Ids of messages an earlier frame published that the transcript has since
   * proven abandoned — a prompt that was re-sent or revised before the agent
   * answered leaves the first submission behind as a dead branch, and the
   * terminal never showed it. Applied BEFORE `messages`. Omitted (not empty)
   * in the common case, so older daemons simply never retract anything.
   */
  supersededMessageIds?: string[];
}

export interface GxserverSessionChatReplacedEvent extends SessionChatFrameBase {
  /** Codex process start in epoch ms; earlier async questions expired on resume. Omitted means unchanged. */
  asyncQuestionsSince?: number | null;
  /** Server-confirmed answers/skips, including answers still queued inside Codex. Omitted means unchanged. */
  retiredAsyncQuestionIds?: string[];
  type: 'sessionChatReplaced';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /**
   * A prompt Claude Code handed back to its composer after an Escape, for the
   * client to put back into its own composer. Applied once per `id` by the
   * client; omitted ⇒ nothing new (never "cleared"). Carried while fresh only.
   */
  returnedPrompt?: SessionChatReturnedPrompt;
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /** Claude's task list from its on-disk store. Omitted ⇒ cleared. */
  agentTasks?: SessionChatAgentTasks;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  accountSwitch?: AccountSwitchProgress | null;
  /** Ghostex's automatic retry after a connection or server error (server/src/accounts/api_errors.rs `chat_state`), or null. */
  apiRetry?: Record<string, unknown> | null;
  pendingModelSelection?: SessionChatPendingModelSelection | null;
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  agentSessionId?: string;
}

export interface GxserverSessionChatStateEvent extends SessionChatFrameBase {
  /** Codex process start in epoch ms; earlier async questions expired on resume. Omitted means unchanged. */
  asyncQuestionsSince?: number | null;
  /** Server-confirmed answers/skips, including answers still queued inside Codex. Omitted means unchanged. */
  retiredAsyncQuestionIds?: string[];
  type: 'sessionChatState';
  status: SessionChatStatus;
  lifecycle?: SessionChatTurnLifecycle;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /**
   * A prompt Claude Code handed back to its composer after an Escape, for the
   * client to put back into its own composer. Applied once per `id` by the
   * client; omitted ⇒ nothing new (never "cleared"). Carried while fresh only.
   */
  returnedPrompt?: SessionChatReturnedPrompt;
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /** Claude's task list from its on-disk store. Omitted ⇒ cleared. */
  agentTasks?: SessionChatAgentTasks;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  accountSwitch?: AccountSwitchProgress | null;
  /** Ghostex's automatic retry after a connection or server error (server/src/accounts/api_errors.rs `chat_state`), or null. */
  apiRetry?: Record<string, unknown> | null;
  pendingModelSelection?: SessionChatPendingModelSelection | null;
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  agentSessionId?: string;
}

export type GxserverSessionChatEvent =
  | GxserverSessionChatSnapshotEvent
  | GxserverSessionChatAppendedEvent
  | GxserverSessionChatReplacedEvent
  | GxserverSessionChatStateEvent;

export function isSessionChatEventType(type: string): type is GxserverSessionChatEvent['type'] {
  return (
    type === 'sessionChatSnapshot' ||
    type === 'sessionChatAppended' ||
    type === 'sessionChatReplaced' ||
    type === 'sessionChatState'
  );
}

// ---------------------------------------------------------------------------
// View mode ("viewMode" is taken by the sidebar layout mode — do not reuse it)
// ---------------------------------------------------------------------------

/*
CDXC:PromptSearch 2026-08-20:
"find" is the Find surface — the GUI for `gx f` — which swaps a session's pane
body on exactly the same terms as chat: the terminal parks rather than closing,
and only one surface can own the pane at a time.
*/
export type SessionSurfaceMode = 'terminal' | 'chat';

export interface SessionChatTerminalDialog {
  id: string;
  title: string;
  body: string;
  footer: string;
  rows: {
    number: number;
    label: string;
    description: string | null;
    selected: boolean;
  }[];
  input: 'search' | 'text' | 'key' | null;
  inputValue: string;
  actions: string[];
}
