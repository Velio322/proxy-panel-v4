-- How the panel reaches the node: 'ws' (agent holds a WebSocket to the panel) or
-- 'pull' (panel polls the agent's mTLS HTTPS API; no persistent session).
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "apiMode" TEXT NOT NULL DEFAULT 'ws';
