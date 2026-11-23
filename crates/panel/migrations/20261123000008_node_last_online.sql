-- Last moment the node was observed ONLINE (WS session, heartbeat or HTTP check).
-- Used as a grace period so short health-check flaps do not drop nodes from /sub/:token.
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "lastOnlineAt" TIMESTAMP(3);

UPDATE "Node"
SET "lastOnlineAt" = COALESCE("lastCheckAt", "updatedAt")
WHERE status = 'ONLINE' AND "lastOnlineAt" IS NULL;
