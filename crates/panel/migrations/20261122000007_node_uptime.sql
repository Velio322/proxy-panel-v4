-- Persist latest reported server/agent uptime (seconds) on Node
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "uptime" BIGINT;
