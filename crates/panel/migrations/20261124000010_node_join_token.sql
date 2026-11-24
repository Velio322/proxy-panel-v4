-- One-time tokens for enrolling a node with a single command (no secret to invent or copy).
CREATE TABLE IF NOT EXISTS "NodeJoinToken" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "tokenHash" TEXT NOT NULL,
  "name" TEXT,
  "createdBy" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "expiresAt" TIMESTAMP(3) NOT NULL,
  "usedAt" TIMESTAMP(3),
  "nodeId" TEXT,
  CONSTRAINT "NodeJoinToken_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX IF NOT EXISTS "NodeJoinToken_tokenHash_key" ON "NodeJoinToken"("tokenHash");
CREATE INDEX IF NOT EXISTS "NodeJoinToken_expiresAt_idx" ON "NodeJoinToken"("expiresAt");
