-- CDXC:SessionChat 2026-10-14 WHY:
-- Where each message `ghostex agents send` left waiting (answered `pending`, `accepted` or
-- `queued`) stands, for the chat card of the session that sent it (agent_message_deliveries.rs).
-- `state` is `waiting` until the recipient's transcript records the message (`delivered`) or
-- Ghostex gives up on it (`failed`, with `failure`). Keyed by the sender and the send's
-- `sendRequestId`; rows are kept 14 days and to the newest 200 settled per sender.
CREATE TABLE agent_message_deliveries (
  senderProjectId TEXT NOT NULL,
  senderSessionId TEXT NOT NULL,
  sendRequestId TEXT NOT NULL,
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  needles TEXT NOT NULL DEFAULT '[]',
  sentAtMs INTEGER NOT NULL,
  state TEXT NOT NULL,
  failure TEXT,
  updatedAtMs INTEGER NOT NULL,
  PRIMARY KEY (senderProjectId, senderSessionId, sendRequestId)
);

CREATE INDEX agent_message_deliveries_waiting ON agent_message_deliveries (state, sentAtMs);

PRAGMA user_version = 45;
