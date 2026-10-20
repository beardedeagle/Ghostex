import type { AccountSwitchProgress } from './agent-accounts';
import type { SessionChatDraftVersion } from './session-chat-queue';
import type { SessionChatDraft, SessionChatQueuedPrompt } from './session-chat-queue';
import type {
  SessionChatMessage,
  SessionChatTurnLifecycle,
  SessionChatStatus,
  SessionChatInteractivePrompt,
  SessionChatQuestionSelection,
} from './session-chat-transcript';
import type {
  SessionChatDetectedOptions,
  SessionChatConversationLock,
  SessionChatTerminalNotice,
  SessionChatTerminalActivity,
  SessionChatReturnedPrompt,
  SessionChatAppCommand,
  SessionChatAgentFleet,
  SessionChatAgentTasks,
} from './session-chat-agent-state';
import type { SessionChatPendingModelSelection } from './session-chat-agents';

// ---------------------------------------------------------------------------
// /api/readSessionChat
// ---------------------------------------------------------------------------

export interface GxserverReadSessionChatParams {
  projectId: string;
  sessionId: string;
  /** Child id or agent name, resolved within this session's descendants. */
  subagent?: string;
  /** Max messages in the tail window. Default 300; page by +200. */
  limit?: number;
  /** Byte offset from a prior page's `beforeOffset` for older history. */
  beforeOffset?: number;
  /**
   * Long-poll (SSH-only clients such as Ghostex mobile): with `fingerprint`,
   * the server holds the request until the chat's fingerprint changes or
   * this many ms elapse (clamped to 30s), then answers with a normal read.
   */
  waitMs?: number;
  /** The `fingerprint` from a previous read result. */
  fingerprint?: string;
}

/**
 * Prefix gxserver stamps on the synthesized divider that marks where stitched
 * scroll-back crosses from one fork ancestor into the next. Mirrors
 * `FORK_BOUNDARY_MESSAGE_ID_PREFIX` in server/src/session_chat_fork_stitch.rs.
 */
export const SESSION_CHAT_FORK_BOUNDARY_ID_PREFIX = 'fork-boundary:';

/** Latest model and effort reported by this child's own transcript. */
export interface SessionChatSubagentInfo {
  id: string;
  name: string;
  agentType?: string;
  model?: string;
  effort?: string;
}

export interface GxserverReadSessionChatResult {
  /** Codex process start in epoch ms; earlier async questions expired on resume. Omitted means unchanged. */
  asyncQuestionsSince?: number | null;
  /** Server-confirmed answers/skips, including answers still queued inside Codex. Omitted means unchanged. */
  retiredAsyncQuestionIds?: string[];
  /** Present only for a child transcript read, independent of the main chat stream. */
  subagent?: SessionChatSubagentInfo;
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  epoch: number;
  seq: number;
  /**
   * Present only when this session's Codex rollout was opened by `codex fork`.
   * `forkedFromId` is the predecessor rollout named by its `session_meta`, and
   * `ancestorIds` walks that lineage oldest-last. Scroll-back is stitched across
   * those files server side, so this is metadata for labelling the boundary, not
   * something a client has to fetch pages with.
   */
  forkInfo?: { forkedFromId: string; ancestorIds: string[] };
  /** Opaque change token for `waitMs` long-polling. */
  fingerprint?: string;
  status: SessionChatStatus;
  agent?: string;
  agentSessionId?: string;
  prompt?: SessionChatInteractivePrompt;
  /**
   * The session's live agent-hook activity: true while the agent is working.
   * Independent of `status` (which describes the transcript read), so a host
   * that only speaks the chat channel still gets the working indicator.
   */
  working?: boolean;
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
   * The session's Ghostex-owned prompt queue, head first. PRESENT (even as an
   * empty array) is the daemon capability probe: a daemon that predates this
   * feature omits it, and a client that sees it omitted hides every queue
   * control instead of calling endpoints that will 404.
   * When present, it is authoritative and replaces the client's list.
   * See CDXC:SessionChat in ./session-chat-queue.
   */
  accountSwitch?: AccountSwitchProgress | null;
  /** Ghostex's automatic retry after a connection or server error (server/src/accounts/api_errors.rs `chat_state`), or null. */
  apiRetry?: Record<string, unknown> | null;
  pendingModelSelection?: SessionChatPendingModelSelection | null;
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Unlike `prompt`/`terminalNotice`, an OMITTED
   * draft means "unchanged / none on the server", NOT cleared — clearing a
   * local draft because an old daemon never sends the field would destroy
   * text the user typed. Clear a draft by writing an empty `content` through
   * /api/setSessionChatDraft instead.
   */
  draft?: SessionChatDraft;
  /**
   * CDXC:Drafts 2026-08-28:
   * The agents this session may still be switched to, resolved by the daemon
   * that owns the project. PRESENT ONLY while the session is a draft: once the
   * first user prompt reaches the agent the session's agent is fixed, so an
   * omitted field is the client's signal to hide the composer's "Agents"
   * section entirely (it is also what a daemon predating drafts sends).
   *
   * The list follows the sidebar Select Agent order and includes its visible
   * agents whose base family is chat-supported, never every launchable agent,
   * because chat cannot read a transcript it has no decoder for.
   */
  availableAgents?: SessionChatAvailableAgent[];
  /**
   * CDXC:AgentProviders 2026-09-03:
   * The same-family agent configurations (accounts) a PROMPTED session can be
   * resumed under, for the composer's "Switch Account" submenu. Absent on
   * drafts, when nothing is compatible, and on daemons predating the feature.
   */
  switchableAgents?: SessionChatAvailableAgent[];
  /**
   * CDXC:Drafts 2026-08-28:
   * The session's own launch agent id, which `agent` above is NOT: that one is
   * the transcript family, so a project custom agent built on Claude reports
   * `claude` there and cannot be told apart from Claude itself. This is the id
   * `/api/switchDraftAgent` takes and the one that matches an `availableAgents`
   * row, so the composer can tick the current agent and — after a switch —
   * follow the new agent without a reload instead of trusting its boot-time URL
   * parameter. Absent on plain terminals and on daemons that predate drafts.
   */
  sessionAgentId?: string;
  error?: string;
}

/**
 * CDXC:Drafts 2026-08-28:
 * One row of the composer's "Agents" section. `agentId` is what
 * `/api/switchDraftAgent` takes; `baseAgentId` is the chat-supported family the
 * agent belongs to (`agentId` itself for a built-in, the custom agent's declared
 * base otherwise) and is what brand-logo and transcript-decoder lookups use.
 */
export interface SessionChatAvailableAgent {
  agentId: string;
  name: string;
  icon: string;
  baseAgentId: string;
}

export type SessionChatSkillSourceKind = 'global' | 'pluginCache' | 'repository';

export interface SessionChatSkill {
  /** Display/mention name, matching the skill folder shown by Agents Hub. */
  name: string;
  /** Absolute skill folder path on the machine that owns this session. */
  directoryPath: string;
  /** Absolute SKILL.md path on the machine that owns this session. */
  skillFilePath: string;
  sourceKind: SessionChatSkillSourceKind;
  /** Present only when gxserver keeps several distinct SKILL.md variations under this name ("project .claude", "plugin plugin-dev 1.2.0"). */
  variantLabel?: string;
}

export interface GxserverReadSessionChatSkillsResult {
  /** gxserver-resolved agent identity; clients do not choose the provider. */
  agentId: string;
  generatedAt: string;
  skills: SessionChatSkill[];
}

/**
 * Composer "@" file mentions. gxserver walks the session's project on its own
 * machine and answers with project-relative paths, so the composer can insert
 * a descriptive Markdown file link that the agent resolves against its working directory.
 */
export interface GxserverReadSessionChatFilesResult {
  /** Absolute project root the paths are relative to. */
  rootPath: string;
  generatedAt: string;
  /** Project-relative paths, always forward-slash separated. */
  files: string[];
  /** True when the walk hit its entry cap, so the list is partial. */
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// /api/sendSessionChatMessage · /api/answerSessionChatPrompt · /api/interruptSessionChat
// ---------------------------------------------------------------------------

/**
 * Raw keystrokes the chat surface can inject into the agent TUI that are not
 * expressible as text. `shift-tab` is Claude Code's permission-mode cycle;
 * shifted arrows adjust Codex reasoning effort.
 */
export type SessionChatSendKey = 'enter' | 'shift-tab' | 'shift-up' | 'shift-down';

export interface GxserverSendSessionChatMessageParams {
  draftVersion?: SessionChatDraftVersion;
  projectId: string;
  sessionId: string;
  /** Message body. Omitted (or empty) when `key` carries the request. */
  text?: string;
  imagePaths?: string[];
  /**
   * Mutually exclusive with `text`/`imagePaths`: writes the key's raw byte
   * sequence into the pty with no bracketed paste, no clear burst and no
   * trailing Enter.
   */
  key?: SessionChatSendKey;
  /**
   * Client-made id (a UUID) naming this send. gxserver delivers one id at
   * most once per session: a retry with the same id answers with the first
   * attempt's result plus `duplicate: true` instead of sending again. Omitted
   * by older clients; gxserver then names the send itself and cannot match
   * that client's retries. The result echoes it as `sendRequestId`.
   */
  sendRequestId?: string;
}

export interface GxserverSendSessionChatMessageResult {
  queued: boolean;
  textBytes: number;
  /** Durable startup queue row, until the agent can accept its first prompt. */
  queuedPromptId?: string;
}

/*
CDXC:Clipboard 2026-08-01:
saveSessionChatImage writes composer-pasted image bytes into the Ghostex image directory on
the machine the session runs on (clients call it over their per-machine RPC,
so a remote session's image lands on the remote machine). The returned
absolute path is what the composer interpolates into "[Image #N](path)" —
the same reference format the terminal paste path produces.
*/
export interface GxserverSaveSessionChatImageParams {
  projectId: string;
  sessionId: string;
  /** Raw base64 or a full data URL (the data: prefix is tolerated). */
  base64Data: string;
  /** Mined only for its extension; the stored name is always generated. */
  suggestedName?: string;
}

export interface GxserverSaveSessionChatImageResult {
  path: string;
  bytes: number;
}

/*
CDXC:SessionChat 2026-08-02:
saveSessionChatAttachment is the non-image sibling of saveSessionChatImage:
any file's bytes land in the Ghostex attachment directory on the session's machine and the
returned absolute path is what the composer interpolates into
"[File #N](path)". The sanitized original file name is kept in the stored
name (after a generated epoch prefix) so agents see a meaningful extension.
*/
export interface GxserverSaveSessionChatAttachmentParams {
  projectId: string;
  sessionId: string;
  /** Raw base64 or a full data URL (the data: prefix is tolerated). */
  base64Data: string;
  /** Creates a dropped directory instead of writing file bytes. */
  directory?: boolean;
  /** Stable, sanitized identity shared by one recursively uploaded folder. */
  uploadId?: string;
  /** Slash-separated path below the recursively uploaded folder root. */
  relativePath?: string;
  /** Sanitized into the stored file name; path segments are stripped. */
  suggestedName?: string;
}

export interface GxserverSaveSessionChatAttachmentResult {
  path: string;
  bytes: number;
}

/*
readSessionChatImage returns the bytes of an image file on the session's
machine (chat-log thumbnails and image links render through it, since the
paths inside "[Image #N](path)" references are machine paths the client
cannot open directly).
*/
export interface GxserverReadSessionChatImageParams {
  /** Absolute path on the machine that serves the RPC. */
  path: string;
}

export interface GxserverReadSessionChatImageResult {
  base64Data: string;
  /** image/* media type inferred from the file's magic bytes / extension. */
  mediaType: string;
  bytes: number;
}

export interface GxserverAnswerSessionChatPromptParams {
  projectId: string;
  sessionId: string;
  /** See `GxserverSendSessionChatMessageParams.sendRequestId`. */
  sendRequestId?: string;
  kind:
    | 'question'
    | 'approval'
    | 'terminalChoice'
    | 'terminalDialog'
    | 'terminalKey'
    | 'asyncQuestion'
    | 'dismissAsyncQuestion'
    | 'recoverCodexConversation'
    | 'trustAndRemember'
    | 'restartAgent';
  conversationLock?: SessionChatConversationLock;
  questionId?: string;
  dialogId?: string;
  dialogAction?: string;
  keyModifiers?: number;
  text?: string;
  /** For questions: one entry per question. */
  selections?: SessionChatQuestionSelection[];
  /** For approvals: the raw byte string of the chosen option ("1" allow, "" deny). */
  approvalSend?: string;
  /**
   * For terminalChoice: the `index` of the `SessionChatTerminalNoticeChoice`
   * the user picked. gxserver re-reads the live screen and walks the highlight
   * onto that row, so a picker that was answered in the terminal meanwhile
   * fails loudly instead of confirming whatever replaced it.
   */
  choiceIndex?: number;
  /** For a `terminalDialog` resume picker: one key (`up`, `down`, `enter`, `escape`, `pageUp`, `pageDown`). */
  pickerKey?: string;
  /** For a resume picker: the row to walk to and pick, matched by title when the rows moved. */
  pickerRow?: { index: number; title: string };
  /** For a resume picker: the search text it should show. */
  pickerQuery?: string;
  /** For `terminalKey`: the key passed through to the agent's blocking screen. */
  key?: string;
}

export interface GxserverAnswerSessionChatPromptResult {
  queued: boolean;
}

export interface GxserverInterruptSessionChatParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverInterruptSessionChatResult {
  interrupted: boolean;
}

export interface GxserverHandoffSessionChatDraftParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverReplaceSessionChatDraftParams {
  projectId: string;
  sessionId: string;
  content: string;
}

export interface GxserverReplaceSessionChatDraftResult {
  replaced: true;
}

/**
 * Result of moving the agent CLI's composer draft out of the terminal so the
 * chat composer can own it. `content` is empty (and `transferred` false) when
 * the CLI composer held nothing — a successful capture of nothing, not an
 * error. The draft is cleared from the terminal before this resolves.
 */
export interface GxserverHandoffSessionChatDraftResult {
  content: string;
  transferred: boolean;
}

// ---------------------------------------------------------------------------
// /api/events frames
// ---------------------------------------------------------------------------

export interface GxserverSubscribeSessionChatMessage {
  type: 'subscribeSessionChat';
  projectId: string;
  sessionId: string;
  /**
   * Follower tail window for snapshot/replaced frames. Hosts pass the size of
   * the list they already display so a re-subscribe (reconnect, duplicate
   * subscribe) cannot answer with fewer rows than are on screen. The server
   * only ever raises a live follower's window, never lowers it; daemons that
   * predate the field ignore it and keep the 300-row default.
   */
  limit?: number;
}

export interface GxserverUnsubscribeSessionChatMessage {
  type: 'unsubscribeSessionChat';
  projectId: string;
  sessionId: string;
}
