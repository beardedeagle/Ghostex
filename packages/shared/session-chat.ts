// Session Chat — normalized chat projection of an agent terminal session.
// Canonical wire types shared by gxserver (Rust mirror in server/src/session_chat.rs)
// and the client hosts; the chat itself reads them through the Rust chat core
// (packages/gx-chat-core).
// All values must stay plain JSON: they cross the /api/events websocket, the CEF bridge,
// and the gpui remote-machine proxy.

// Ghostex's own prompt queue and the synced composer draft live in
// ./session-chat-queue (canonical) and are re-exported here so consumers keep
// a single import surface. Do NOT confuse SessionChatQueuedPrompt with
// SessionChatMessage.queued — see the note on that field below.
export type {
  GxserverQueueSessionChatPromptParams,
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatQueueParams,
  GxserverReadSessionChatQueueResult,
  GxserverRemoveSessionChatQueuedPromptParams,
  GxserverReorderSessionChatQueueParams,
  GxserverSendSessionChatQueuedPromptParams,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
  GxserverSetSessionChatDraftParams,
  GxserverSetSessionChatDraftResult,
  GxserverUpdateSessionChatQueuedPromptParams,
  SessionChatDraft,
  SessionChatQueuedPrompt,
  SessionChatQueuedPromptState,
} from './session-chat-queue';
export * from './session-chat-agents';
export * from './session-chat-transcript';
export * from './session-chat-agent-state';
export * from './session-chat-rpc';
export * from './session-chat-events';
