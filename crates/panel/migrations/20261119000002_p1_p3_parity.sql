-- ProxyPanel v4 extended tables, columns, and indexes

-- Ensure new Protocol enum values exist even on upgraded databases
ALTER TYPE "Protocol" ADD VALUE IF NOT EXISTS 'ANYTLS';
ALTER TYPE "Protocol" ADD VALUE IF NOT EXISTS 'SHADOWTLS';

-- User: 2FA TOTP & Telegram OAuth
ALTER TABLE "User" ADD COLUMN IF NOT EXISTS "totpSecret" TEXT;
ALTER TABLE "User" ADD COLUMN IF NOT EXISTS "totpEnabled" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "User" ADD COLUMN IF NOT EXISTS "telegramId" BIGINT;
CREATE UNIQUE INDEX IF NOT EXISTS "User_telegramId_key" ON "User"("telegramId");

-- Plan: HWID device limit & default reset strategy
ALTER TABLE "Plan" ADD COLUMN IF NOT EXISTS "hwidLimit" INTEGER;
ALTER TABLE "Plan" ADD COLUMN IF NOT EXISTS "defaultResetStrategy" TEXT NOT NULL DEFAULT 'NO_RESET';

-- Client: Plan association, HWID limit, flexible traffic reset strategy, and ON_HOLD support
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "planId" TEXT;
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "deviceLimit" INTEGER;
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "resetStrategy" TEXT NOT NULL DEFAULT 'NO_RESET';
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "onHold" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "onHoldDurationDays" INTEGER;
ALTER TABLE "Client" ADD COLUMN IF NOT EXISTS "activatedAt" TIMESTAMP(3);
CREATE INDEX IF NOT EXISTS "Client_planId_idx" ON "Client"("planId");

-- Node: Indexed secretHash lookup and mTLS certificate fields
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "secretHash" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "mtlsCertPem" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "mtlsSerial" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "mtlsFingerprint" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "mtlsExpiresAt" TIMESTAMP(3);
CREATE INDEX IF NOT EXISTS "Node_secretHash_idx" ON "Node"("secretHash");

-- Device (HWID device enforcement)
CREATE TABLE IF NOT EXISTS "Device" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT NOT NULL,
  "hwid" TEXT NOT NULL,
  "os" TEXT,
  "osVersion" TEXT,
  "model" TEXT,
  "userAgent" TEXT,
  "ip" TEXT,
  "lastSeenAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Device_pkey" PRIMARY KEY ("id"),
  CONSTRAINT "Device_clientId_fkey" FOREIGN KEY ("clientId") REFERENCES "Client"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "Device_clientId_hwid_key" ON "Device"("clientId","hwid");
CREATE INDEX IF NOT EXISTS "Device_clientId_lastSeenAt_idx" ON "Device"("clientId","lastSeenAt");

-- Webhook (HMAC-SHA256 webhooks)
CREATE TABLE IF NOT EXISTS "Webhook" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "url" TEXT NOT NULL,
  "secret" TEXT NOT NULL DEFAULT '',
  "scopes" TEXT[] NOT NULL DEFAULT '{}',
  "events" TEXT[] NOT NULL DEFAULT '{}',
  "active" BOOLEAN NOT NULL DEFAULT true,
  "lastStatus" INTEGER,
  "lastError" TEXT,
  "lastDeliveredAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Webhook_pkey" PRIMARY KEY ("id")
);

-- Host (Unified per-inbound Host overrides)
CREATE TABLE IF NOT EXISTS "Host" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "inboundId" TEXT,
  "remark" TEXT NOT NULL,
  "address" TEXT NOT NULL,
  "port" INTEGER,
  "sni" TEXT,
  "host" TEXT,
  "path" TEXT,
  "alpn" TEXT,
  "fingerprint" TEXT,
  "xhttpExtra" JSONB,
  "finalMask" JSONB,
  "vlessRouteId" TEXT,
  "shuffleHost" BOOLEAN NOT NULL DEFAULT false,
  "fragment" JSONB,
  "isCdn" BOOLEAN NOT NULL DEFAULT false,
  "sortOrder" INTEGER NOT NULL DEFAULT 0,
  "enable" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Host_pkey" PRIMARY KEY ("id"),
  CONSTRAINT "Host_inboundId_fkey" FOREIGN KEY ("inboundId") REFERENCES "Inbound"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE INDEX IF NOT EXISTS "Host_inboundId_enable_idx" ON "Host"("inboundId","enable");

-- SubscriptionTemplate
CREATE TABLE IF NOT EXISTS "SubscriptionTemplate" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "templateType" TEXT NOT NULL,
  "content" TEXT NOT NULL,
  "isDefault" BOOLEAN NOT NULL DEFAULT false,
  "description" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "SubscriptionTemplate_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "SubscriptionTemplate_templateType_isDefault_idx" ON "SubscriptionTemplate"("templateType","isDefault");

-- SrrRule (Subscription Request Rules + age encryption)
CREATE TABLE IF NOT EXISTS "SrrRule" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "enabled" BOOLEAN NOT NULL DEFAULT true,
  "priority" INTEGER NOT NULL DEFAULT 0,
  "matchHeader" TEXT NOT NULL DEFAULT 'user-agent',
  "matchOperator" TEXT NOT NULL DEFAULT 'CONTAINS',
  "matchPattern" TEXT NOT NULL DEFAULT '',
  "negate" BOOLEAN NOT NULL DEFAULT false,
  "conditions" JSONB NOT NULL DEFAULT '[]',
  "action" TEXT NOT NULL DEFAULT 'ALLOW',
  "templateId" TEXT,
  "overrideFormat" TEXT,
  "customRemarks" TEXT,
  "agePublicKey" TEXT,
  "ageConfig" JSONB,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "SrrRule_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "SrrRule_enabled_priority_idx" ON "SrrRule"("enabled","priority");

-- SubRequestHistory
CREATE TABLE IF NOT EXISTS "SubRequestHistory" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT,
  "username" TEXT,
  "ip" TEXT NOT NULL,
  "userAgent" TEXT NOT NULL,
  "detectedApp" TEXT,
  "format" TEXT,
  "srrRuleName" TEXT,
  "status" INTEGER NOT NULL DEFAULT 200,
  "hwid" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "SubRequestHistory_pkey" PRIMARY KEY ("id")
);
CREATE INDEX IF NOT EXISTS "SubRequestHistory_clientId_createdAt_idx" ON "SubRequestHistory"("clientId","createdAt" DESC);
CREATE INDEX IF NOT EXISTS "SubRequestHistory_createdAt_idx" ON "SubRequestHistory"("createdAt" DESC);

-- WebAuthnCredential (Passkeys)
CREATE TABLE IF NOT EXISTS "WebAuthnCredential" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "userId" TEXT NOT NULL,
  "credentialId" TEXT NOT NULL,
  "name" TEXT,
  "passkeyJson" JSONB NOT NULL,
  "lastUsedAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "WebAuthnCredential_pkey" PRIMARY KEY ("id"),
  CONSTRAINT "WebAuthnCredential_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "WebAuthnCredential_credentialId_key" ON "WebAuthnCredential"("credentialId");
CREATE INDEX IF NOT EXISTS "WebAuthnCredential_userId_idx" ON "WebAuthnCredential"("userId");

-- InfraProvider & InfraNodeBilling
CREATE TABLE IF NOT EXISTS "InfraProvider" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "website" TEXT,
  "supportUrl" TEXT,
  "notes" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "InfraProvider_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "InfraNodeBilling" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "nodeId" TEXT NOT NULL,
  "providerId" TEXT NOT NULL,
  "amount" DECIMAL(12,2) NOT NULL DEFAULT 0,
  "currency" TEXT NOT NULL DEFAULT 'USD',
  "billingCycle" TEXT NOT NULL DEFAULT 'MONTHLY',
  "nextDueDate" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "autoRenew" BOOLEAN NOT NULL DEFAULT true,
  "notes" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "InfraNodeBilling_pkey" PRIMARY KEY ("id"),
  CONSTRAINT "InfraNodeBilling_nodeId_fkey" FOREIGN KEY ("nodeId") REFERENCES "Node"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "InfraNodeBilling_providerId_fkey" FOREIGN KEY ("providerId") REFERENCES "InfraProvider"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE INDEX IF NOT EXISTS "InfraNodeBilling_nodeId_idx" ON "InfraNodeBilling"("nodeId");
CREATE INDEX IF NOT EXISTS "InfraNodeBilling_providerId_idx" ON "InfraNodeBilling"("providerId");
