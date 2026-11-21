ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "allNodes" BOOLEAN NOT NULL DEFAULT true;

CREATE TABLE IF NOT EXISTS "ClientNode" (
    "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
    "clientId" TEXT NOT NULL,
    "nodeId" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "ClientNode_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "ClientNode_clientId_nodeId_key" UNIQUE ("clientId", "nodeId"),
    CONSTRAINT "ClientNode_clientId_fkey" FOREIGN KEY ("clientId") REFERENCES "Client"("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "ClientNode_nodeId_fkey" FOREIGN KEY ("nodeId") REFERENCES "Node"("id") ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX IF NOT EXISTS "ClientNode_clientId_idx" ON "ClientNode"("clientId");
CREATE INDEX IF NOT EXISTS "ClientNode_nodeId_idx" ON "ClientNode"("nodeId");
