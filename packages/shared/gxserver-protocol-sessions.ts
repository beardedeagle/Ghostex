import type { SessionChatDraftVersion } from './session-chat-queue';
import type { GxserverProjectId, GxserverSessionId } from './gxserver-protocol-core';
import type {
  GxserverAgentStartupTextDisposition,
  GxserverAgentResumePlan,
  GxserverStartSessionProviderResult,
} from './gxserver-protocol-session-runtime';
import type { GxserverSessionDomainState } from './gxserver-protocol-domain';

/*
CDXC:Worktrees 2026-07-29:
Sidebar V2's worktree flow. A worktree is an ATTRIBUTE of a session (its cwd
plus branch), not a registered sibling project, so ONE call creates the
checkout and the session that lives in it, atomically, server-side.

Contract rules the emitter and every client agree on:
- `projectId` is the PARENT project the worktree is cut from. gxserver derives
  the checkout path, the temp branch (`ghostex/<8hex>`), and the setup command
  from that project; the client never sends paths it invented.
- `baseBranch` omitted means the repository's default branch. `startFromOrigin`
  asks gxserver to fetch first and branch from `origin/<baseBranch>` instead of
  the local ref, so a stale local branch cannot silently seed the worktree.
- `existingWorktree.path` SKIPS creation entirely and spawns the session inside
  that checkout. The path must come from gxserver's own worktree list (or an
  existing session's cwd); gxserver re-validates and normalizes it.
- `firstPrompt` is optional. Without it the session starts idle in the agent,
  exactly like a plain agent launch with no prompt.
- The whole sequence rolls back (worktree removed) if any step fails, so a
  failed call leaves no half-made checkout behind.
*/
export interface GxserverCreateWorktreeSessionExistingWorktree {
  /** Absolute path to an existing checkout on the daemon's machine. */
  path: string;
}

export interface GxserverCreateWorktreeSessionParams {
  agentId?: string;
  baseBranch?: string;
  existingWorktree?: GxserverCreateWorktreeSessionExistingWorktree;
  firstPrompt?: string;
  projectId: GxserverProjectId;
  startFromOrigin?: boolean;
}

export interface GxserverCreateWorktreeSessionResult {
  /** The branch the session's checkout is on — the temp `ghostex/<8hex>` for a
      fresh worktree, or whatever the existing checkout was already on. */
  branch: string;
  sessionId: GxserverSessionId;
  worktreePath: string;
}

/*
CDXC:Worktrees 2026-07-29:
Cleanup for a worktree whose last session just closed. gxserver checks the
checkout for uncommitted work FIRST: `dirty: true` with `removed: false` means
it refused and the client must re-ask with `force`. `warnings` carries bounded,
user-safe notes (a branch that could not be deleted, for instance), never raw
git output.
*/
export interface GxserverRemoveSessionWorktreeParams {
  force?: boolean;
  projectId: GxserverProjectId;
  worktreePath: string;
}

export interface GxserverRemoveSessionWorktreeResult {
  dirty?: boolean;
  removed: boolean;
  warnings?: readonly string[];
}

export interface GxserverForkSessionParams extends GxserverSessionLifecycleParams {}

export interface GxserverAgentForkPlan {
  agentId?: string;
  baseCommand?: string;
  displayCommand?: string;
  primaryCommand?: string;
  runtimeCommand?: string;
  startupText?: string;
  startupTextDisposition: GxserverAgentStartupTextDisposition;
}

export interface GxserverForkSessionResult {
  plan: GxserverAgentForkPlan;
  provider?: GxserverStartSessionProviderResult;
  session: GxserverSessionDomainState;
  sourceSession: GxserverSessionDomainState;
}

/*
CDXC:Drafts 2026-08-28:
Agent switching is DRAFTS ONLY. A draft has no conversation, so swapping its
agent needs no confirmation and loses nothing: gxserver kills the draft's
background CLI, clears the session's agent identity, rebuilds its launch plan
with the same resolution `/api/createAgentSession` uses, and starts the new
agent's CLI. The call is refused with `invalidState` once the session has been
promoted (its first user prompt reached the agent), because at that point the
transcript, the resume plan, and the session's agent identity all belong to the
agent that produced them.

`agentId` is a visible sidebar project agent whose base family is
chat-supported. Clients read the allowed set from `availableAgents` on
`/api/readSessionChat` rather than building it themselves.
*/
export interface GxserverSwitchDraftAgentParams extends GxserverSessionLifecycleParams {
  agentId: string;
}

export interface GxserverSwitchDraftAgentResult {
  agentId: string;
  /** The provider start for the new agent's CLI, when one ran. */
  provider?: GxserverStartSessionProviderResult;
  session: GxserverSessionDomainState;
}

/*
CDXC:AgentProviders 2026-09-03:
`/api/switchSessionAgent` rewrites which agent configuration a PROMPTED session
launches with, keeping its provider conversation. It does not cycle the
provider: the caller runs Full Reload (sleep, then wake) afterwards, and the
wake's resume plan carries the new agent's command. `agentId` must be one of
the rows the daemon published in the session's `switchableAgents`.
*/
export interface GxserverSwitchSessionAgentParams extends GxserverSessionLifecycleParams {
  agentId: string;
}

export interface GxserverSwitchSessionAgentResult {
  agentId: string;
  plan: GxserverAgentResumePlan;
  session: GxserverSessionDomainState;
}

/** One account a session can be moved to; the same shape as chat's `availableAgents` rows. */
export interface GxserverSwitchableSessionAgent {
  agentId: string;
  baseAgentId: string;
  icon: string;
  name: string;
}

/*
CDXC:SessionChat 2026-08-26:
readSessionTerminalTail answers "is the agent CLI's input box on screen, and if
not, what IS on screen". The daemon reads the same capture every other
screen-state reader takes (a direct zmx socket read, single-digit milliseconds)
and returns the bottom of it.

`composerState` is deliberately three-valued and `unknown` is the common case,
not an error: the daemon has measured composer signatures for nine agent CLIs
and answers `unknown` for every other agent, and for any capture it could not
read. Nothing may treat `unknown` as "not ready" — the daemon itself fails open
on it, and a client that did otherwise would block sends the daemon allows.
*/
export type GxserverSessionComposerState = 'ready' | 'notReady' | 'unknown';

export interface GxserverReadSessionTerminalTailParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /** Overrides the agent the daemon resolves from the session row. */
  agentId?: string;
}

export interface GxserverReadSessionTerminalTailResult {
  /** The agent id the verdict was computed for, when one could be resolved. */
  agentId: string | null;
  /** False when no whole screen could be read; `lines` is then empty. */
  captured: boolean;
  composerState: GxserverSessionComposerState;
  /**
   * Up to 30 ANSI-stripped physical screen rows, OLDEST FIRST, preserving
   * indentation, blank rows, and box-drawing runs. Empty bottom padding is
   * omitted, and the newest painted row is last.
   */
  lines: readonly string[];
  /** User-facing sentence, present only for `notReady`. */
  reason: string | null;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

/*
CDXC:TranscriptExport 2026-08-20:
exportSessionTranscript renders the session's agent transcript into a markdown
file so a NEW agent conversation can be started with that file mentioned. The
transcript only exists on the machine that runs the agent, so clients call this
over their per-machine RPC and the returned path is absolute ON THAT MACHINE —
a remote session's export never lands on the client's disk. The daemon owns the
destination (`<app data dir>/exports`); the caller cannot name a path.

Failures are structured errors, not a degraded export: `unsupportedAgent` (the
session's agent has no transcript format Ghostex parses), `invalidParams` (the
session has not reported an agent session id yet), `transcriptNotFound`,
`transcriptUnreadable` and `transcriptEmpty`.
*/
export interface GxserverExportSessionTranscriptParams {
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  /**
   * CDXC:TranscriptExport 2026-08-24:
   * The export dialog's include-toggles. User and agent messages are never
   * optional; these govern the optional record families. Absent values keep
   * the daemon's historical defaults (commands and patches in, reasoning
   * out), which is also what daemons predating the toggles export.
   */
  includeCommands?: boolean;
  includePatches?: boolean;
  includeReasoning?: boolean;
}

export interface GxserverExportSessionTranscriptResult {
  /** Absolute path of the written markdown file, on the daemon's machine. */
  path: string;
  bytes: number;
  /** The transcript the export was parsed from, on the daemon's machine. */
  sourcePath: string;
  /** Records the export actually rendered, after the section selection. */
  renderedEntries: number;
  /** Records parsed out of the transcript, whether rendered or not. */
  parsedEntries: number;
  /** The session's agent (`claude`, `codex`, `grok`, `pi`, …). */
  agent?: string;
}

export interface GxserverUpdateSessionOrderParams {
  projectId: GxserverProjectId;
  sessionIds: readonly GxserverSessionId[];
}

export interface GxserverUpdateSessionOrderResult {
  sessions: readonly GxserverSessionDomainState[];
}

export interface GxserverRemoveSessionParams {
  projectId: GxserverProjectId;
  /** Why the session is being removed. Free-form and ignored by gxserver. */
  reason?: string;
  sessionId: GxserverSessionId;
}

export interface GxserverRemoveSessionResult {
  session: GxserverSessionDomainState;
}

export interface GxserverSessionLifecycleParams {
  projectId: GxserverProjectId;
  reason?: string;
  sessionId: GxserverSessionId;
}

/**
 * CDXC:Drafts 2026-08-28:
 * One row per session with unsent composer text, from `/api/listSessionChatDrafts`.
 * Clients reconcile their per-keystroke draft cache from this at boot, because
 * that cache does not survive a kill that skips a clean Chromium shutdown —
 * the daemon's SQLite copy does. `updatedAt` is the daemon's ISO stamp of the
 * last synced push, which is what the reconcile compares against.
 */
export interface GxserverSessionChatDraftListEntry {
  parked?: boolean;
  deliveredDrafts?: import('./session-chat-queue').SessionChatDeliveredDraft[];
  recoveryDrafts?: import('./session-chat-queue').SessionChatRecoveryDraft[];
  version?: SessionChatDraftVersion;
  consumedDrafts?: SessionChatDraftVersion[];
  originClientId?: string;
  content: string;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
  updatedAt: string;
}

export interface GxserverListSessionChatDraftsResult {
  drafts: GxserverSessionChatDraftListEntry[];
  recoveryDrafts?: import('./session-chat-queue').SessionChatRecoveryDraft[];
}

/*
CDXC:KeepAwake 2026-08-19:
A sleep request says WHO asked. `"automatic"` marks a client's "Sleep inactive
agents" sweep; anything else (including an absent field, which is what every
caller sent before this existed) is a user action.

Automatic sleeps can additionally be declined by a live keep-awake lease (see
`GxserverHoldSessionsAwakeParams`) or a never-active session. Every sleep is
declined when the target is no longer running, because stopped history must not
be promoted into the active sleeping lifecycle.
*/
export type GxserverSleepTrigger = 'automatic' | 'user';

export interface GxserverSleepSessionParams extends GxserverSessionLifecycleParams {
  sleepTrigger?: GxserverSleepTrigger;
}

export interface GxserverSleepSessionResult {
  /*
  Present ONLY when the daemon refused the request. `"keptAwake"` means an
  automatic sweep hit a session another client is attached to; `"neverActive"`
  means it hit a session nobody has prompted yet, which has no idle time to
  measure and no conversation to resume; `"backgroundWork"` and
  `"pendingQuestion"` mean it hit a session whose agent still runs a background
  shell or waits on an unanswered question; `"notRunning"` means a stale client
  targeted sleeping or stopped history. In every case the session was not
  touched, so a client must not optimistically mark the row sleeping.
  */
  declined?: 'backgroundWork' | 'keptAwake' | 'neverActive' | 'notRunning' | 'pendingQuestion';
  kill?: Record<string, unknown>;
  session: GxserverSessionDomainState;
}

/*
CDXC:KeepAwake 2026-08-19:
A client that is ATTACHED to sessions it does not own panes for — Ghostex mobile
over its SSH CLI bridge — renews a keep-awake lease so the machine's Auto Sleep
sweep cannot retire a terminal the user is looking at on another device.

Contract:
- Leases are in-memory and TTL-bounded on the daemon. Renew well inside `ttlMs`;
  stop renewing and the hold lapses on its own. There is no required release.
- `holderId` scopes the lease to one device, so two phones on one session cannot
  release each other's hold. Absent means a shared default holder.
- `release: true` drops this holder's leases for the listed sessions instead of
  extending them (used when a tab closes, so the session becomes sleepable again
  without waiting out the TTL).
- Ids that do not resolve on that daemon come back in `unknownSessions` instead
  of failing the call: one killed session must not stop the other tabs' holds.
*/
export interface GxserverHoldSessionsAwakeParams {
  holderId?: string;
  release?: boolean;
  sessions: readonly {
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
  ttlMs?: number;
}

export interface GxserverHoldSessionsAwakeResult {
  holderId: string;
  released: boolean;
  sessions: {
    keepAwakeUntil?: string;
    keptAwake: boolean;
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
  /** The TTL actually applied after the daemon clamped it. */
  ttlMs: number;
  unknownSessions: {
    projectId: GxserverProjectId;
    sessionId: GxserverSessionId;
  }[];
}

export type GxserverSessionTransitionAction = 'close' | 'sleep';
export interface GxserverSessionTransitionParams extends GxserverSessionLifecycleParams {
  /*
  CDXC:StateSync 2026-06-02-13:01:
  gxserver owns the shared lifecycle mutation for close/sleep, but macOS owns selected tab and local pane focus. Keep visual order and focus-target selection out of this protocol so pane-tab layout cannot become gxserver-owned state.
  */
  action: GxserverSessionTransitionAction;
}

export interface GxserverSessionTransitionResult {
  action: GxserverSessionTransitionAction;
  declined?: 'notRunning';
  session: GxserverSessionDomainState;
  transition: Record<string, unknown> & {
    session: GxserverSessionDomainState;
  };
}

export interface GxserverCancelFirstPromptAutoTitleParams extends GxserverSessionLifecycleParams {}

export interface GxserverCancelFirstPromptAutoTitleResult {
  changed: boolean;
  previousStatus?: string;
  reason: string;
  session: GxserverSessionDomainState;
}
