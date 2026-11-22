-- ProxyPanel v4 baseline schema

-- Enums
DO $$ BEGIN CREATE TYPE "UserRole" AS ENUM ('SUPER_ADMIN','ADMIN','RESELLER','OPERATOR'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "PlanType" AS ENUM ('USER','RESELLER'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "SubFormat" AS ENUM ('BASE64','JSON','CLASH','SINGBOX','ORIGINAL'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "NodeStatus" AS ENUM ('ONLINE','OFFLINE','ERROR','MAINTENANCE'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "Protocol" AS ENUM ('VLESS','VMESS','TROJAN','SHADOWSOCKS','HYSTERIA2','NAIVEPROXY','MIERU','TUIC','ANYTLS','SHADOWTLS'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "NotificationType" AS ENUM ('INFO','WARNING','ERROR','SUCCESS','TRAFFIC_LIMIT','SUBSCRIPTION_EXPIRED','NODE_OFFLINE','PAYMENT_RECEIVED'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "BackupType" AS ENUM ('DATABASE','CONFIG','FULL'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE TYPE "BackupStatus" AS ENUM ('SUCCESS','FAILED','IN_PROGRESS'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- Tables
CREATE TABLE IF NOT EXISTS "Reseller" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "contactEmail" TEXT,
  "contactTelegram" TEXT,
  "maxClients" INTEGER NOT NULL DEFAULT 100,
  "maxNodes" INTEGER NOT NULL DEFAULT 5,
  "trafficLimit" BIGINT NOT NULL DEFAULT 107374182400,
  "usedTraffic" BIGINT NOT NULL DEFAULT 0,
  "balance" DECIMAL(12,2) NOT NULL DEFAULT 0,
  "whiteLabel" JSONB,
  "active" BOOLEAN NOT NULL DEFAULT true,
  "expiresAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Reseller_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "User" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "username" TEXT NOT NULL,
  "email" TEXT,
  "password" TEXT NOT NULL,
  "role" "UserRole" NOT NULL DEFAULT 'ADMIN',
  "resellerId" TEXT,
  "language" TEXT NOT NULL DEFAULT 'en',
  "lastLoginAt" TIMESTAMP(3),
  "lastLoginIp" TEXT,
  "banned" BOOLEAN NOT NULL DEFAULT false,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "User_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "UserSession" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "userId" TEXT NOT NULL,
  "token" TEXT NOT NULL,
  "ip" TEXT,
  "userAgent" TEXT,
  "expiresAt" TIMESTAMP(3) NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "UserSession_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "ApiKey" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "userId" TEXT NOT NULL,
  "key" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "permissions" JSONB NOT NULL DEFAULT '[]',
  "expiresAt" TIMESTAMP(3),
  "lastUsedAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ApiKey_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Plan" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "description" TEXT,
  "type" "PlanType" NOT NULL DEFAULT 'USER',
  "price" DECIMAL(10,2) NOT NULL DEFAULT 0,
  "currency" TEXT NOT NULL DEFAULT 'USD',
  "duration" INTEGER NOT NULL DEFAULT 30,
  "trafficLimit" BIGINT NOT NULL DEFAULT 0,
  "maxClients" INTEGER,
  "maxSpeed" INTEGER,
  "protocols" JSONB NOT NULL DEFAULT '[]',
  "sortOrder" INTEGER NOT NULL DEFAULT 0,
  "active" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Plan_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Client" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "uuid" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "username" TEXT NOT NULL,
  "email" TEXT,
  "password" TEXT NOT NULL,
  "trafficLimit" BIGINT NOT NULL DEFAULT 0,
  "usedTraffic" BIGINT NOT NULL DEFAULT 0,
  "uploadTraffic" BIGINT NOT NULL DEFAULT 0,
  "downloadTraffic" BIGINT NOT NULL DEFAULT 0,
  "expireAt" TIMESTAMP(3),
  "banned" BOOLEAN NOT NULL DEFAULT false,
  "restrictionMode" TEXT NOT NULL DEFAULT 'SOFT',
  "restrictionReason" TEXT,
  "restrictedAt" TIMESTAMP(3),
  "trafficResetDay" INTEGER,
  "nextTrafficResetAt" TIMESTAMP(3),
  "lastTrafficResetAt" TIMESTAMP(3),
  "accessRevision" INTEGER NOT NULL DEFAULT 0,
  "note" TEXT,
  "telegramId" BIGINT,
  "subId" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "subToken" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "protocols" JSONB NOT NULL DEFAULT '["VLESS","HYSTERIA2"]',
  "lastActiveAt" TIMESTAMP(3),
  "lastLoginIp" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "resellerId" TEXT,
  CONSTRAINT "Client_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "ClientSetting" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT NOT NULL,
  "subFormat" "SubFormat" NOT NULL DEFAULT 'BASE64',
  "autoUpdate" BOOLEAN NOT NULL DEFAULT true,
  "customSni" TEXT,
  "settings" JSONB NOT NULL DEFAULT '{}',
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ClientSetting_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Node" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "host" TEXT NOT NULL,
  "port" INTEGER NOT NULL DEFAULT 443,
  "apiPort" INTEGER NOT NULL DEFAULT 2222,
  "secret" TEXT NOT NULL,
  "country" TEXT,
  "city" TEXT,
  "isp" TEXT,
  "status" "NodeStatus" NOT NULL DEFAULT 'OFFLINE',
  "lastCheckAt" TIMESTAMP(3),
  "lastPingMs" INTEGER,
  "version" TEXT,
  "cpuUsage" DOUBLE PRECISION,
  "memUsage" DOUBLE PRECISION,
  "netUpload" BIGINT NOT NULL DEFAULT 0,
  "netDownload" BIGINT NOT NULL DEFAULT 0,
  "xrayVersion" TEXT,
  "singboxVersion" TEXT,
  "features" JSONB NOT NULL DEFAULT '{}',
  "tags" TEXT[] NOT NULL DEFAULT '{}',
  "active" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Node_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "NodeMetric" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "nodeId" TEXT NOT NULL,
  "cpuUsage" DOUBLE PRECISION,
  "memUsage" DOUBLE PRECISION,
  "netUpload" BIGINT NOT NULL DEFAULT 0,
  "netDownload" BIGINT NOT NULL DEFAULT 0,
  "connections" INTEGER NOT NULL DEFAULT 0,
  "uptime" INTEGER NOT NULL DEFAULT 0,
  "tcpConnections" INTEGER,
  "udpSockets" INTEGER,
  "uploadBps" DOUBLE PRECISION,
  "downloadBps" DOUBLE PRECISION,
  "hostTelemetry" JSONB,
  "coreStatuses" JSONB,
  "recordedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "NodeMetric_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Inbound" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "nodeId" TEXT NOT NULL,
  "protocol" "Protocol" NOT NULL,
  "tag" TEXT NOT NULL,
  "port" INTEGER NOT NULL,
  "listen" TEXT NOT NULL DEFAULT '0.0.0.0',
  "settings" JSONB NOT NULL,
  "stream" JSONB NOT NULL DEFAULT '{}',
  "routing" JSONB NOT NULL DEFAULT '{}',
  "sniffing" BOOLEAN NOT NULL DEFAULT true,
  "remark" TEXT,
  "enable" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Inbound_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "PortShare" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "inboundId" TEXT NOT NULL,
  "protocol" "Protocol" NOT NULL,
  "tag" TEXT NOT NULL,
  "settings" JSONB NOT NULL DEFAULT '{}',
  "stream" JSONB NOT NULL DEFAULT '{}',
  "host" TEXT,
  "path" TEXT,
  "enable" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "PortShare_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "ClientInbound" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT NOT NULL,
  "inboundId" TEXT NOT NULL,
  "uuid" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "enable" BOOLEAN NOT NULL DEFAULT true,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ClientInbound_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "RoutingRule" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "name" TEXT NOT NULL,
  "description" TEXT,
  "enabled" BOOLEAN NOT NULL DEFAULT true,
  "priority" INTEGER NOT NULL DEFAULT 0,
  "type" TEXT NOT NULL DEFAULT 'field',
  "domain" TEXT[] NOT NULL DEFAULT '{}',
  "ip" TEXT[] NOT NULL DEFAULT '{}',
  "port" TEXT,
  "sourcePort" TEXT,
  "source" TEXT[] NOT NULL DEFAULT '{}',
  "protocol" TEXT[] NOT NULL DEFAULT '{}',
  "inboundTag" TEXT[] NOT NULL DEFAULT '{}',
  "outboundTag" TEXT NOT NULL DEFAULT 'direct',
  "balancerTag" TEXT,
  "subRules" JSONB,
  "nodeScope" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "RoutingRule_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "TrafficLog" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT NOT NULL,
  "nodeId" TEXT NOT NULL,
  "upload" BIGINT NOT NULL DEFAULT 0,
  "download" BIGINT NOT NULL DEFAULT 0,
  "protocol" "Protocol",
  "inboundTag" TEXT,
  "recordAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "TrafficLog_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "AuditLog" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "userId" TEXT,
  "action" TEXT NOT NULL,
  "resource" TEXT NOT NULL,
  "resourceId" TEXT,
  "details" JSONB,
  "ip" TEXT,
  "userAgent" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "AuditLog_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "TelegramUser" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "telegramId" BIGINT NOT NULL,
  "username" TEXT,
  "firstName" TEXT,
  "lastName" TEXT,
  "language" TEXT NOT NULL DEFAULT 'en',
  "isSubscribed" BOOLEAN NOT NULL DEFAULT false,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "TelegramUser_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Notification" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "type" "NotificationType" NOT NULL,
  "title" TEXT NOT NULL,
  "message" TEXT NOT NULL,
  "targetUserId" TEXT,
  "targetResellerId" TEXT,
  "read" BOOLEAN NOT NULL DEFAULT false,
  "metadata" JSONB,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "Notification_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "SystemSetting" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "key" TEXT NOT NULL,
  "value" JSONB NOT NULL,
  "description" TEXT,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "SystemSetting_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "BackupLog" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "type" "BackupType" NOT NULL,
  "status" "BackupStatus" NOT NULL DEFAULT 'SUCCESS',
  "destination" TEXT NOT NULL,
  "filePath" TEXT,
  "fileSize" BIGINT,
  "error" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "BackupLog_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "Translation" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "locale" TEXT NOT NULL,
  "key" TEXT NOT NULL,
  "value" TEXT NOT NULL,
  CONSTRAINT "Translation_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "ProtocolCredential" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "clientId" TEXT NOT NULL,
  "protocol" "Protocol" NOT NULL,
  "encryptedSecret" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProtocolCredential_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "TrafficReceipt" (
  "id" TEXT NOT NULL,
  "batchId" TEXT,
  "nodeId" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "TrafficReceipt_pkey" PRIMARY KEY ("id")
);

CREATE TABLE IF NOT EXISTS "NodeCommand" (
  "id" TEXT NOT NULL DEFAULT gen_random_uuid()::text,
  "nodeId" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
  "payload" JSONB NOT NULL,
  "revision" INTEGER NOT NULL DEFAULT 0,
  "status" TEXT NOT NULL DEFAULT 'PENDING',
  "attempts" INTEGER NOT NULL DEFAULT 0,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "acknowledgedAt" TIMESTAMP(3),
  "lastError" TEXT,
  CONSTRAINT "NodeCommand_pkey" PRIMARY KEY ("id")
);

-- Unique indexes
CREATE UNIQUE INDEX IF NOT EXISTS "User_username_key" ON "User"("username");
CREATE UNIQUE INDEX IF NOT EXISTS "User_email_key" ON "User"("email");
CREATE UNIQUE INDEX IF NOT EXISTS "UserSession_token_key" ON "UserSession"("token");
CREATE UNIQUE INDEX IF NOT EXISTS "ApiKey_key_key" ON "ApiKey"("key");
CREATE UNIQUE INDEX IF NOT EXISTS "Client_uuid_key" ON "Client"("uuid");
CREATE UNIQUE INDEX IF NOT EXISTS "Client_telegramId_key" ON "Client"("telegramId");
CREATE UNIQUE INDEX IF NOT EXISTS "Client_subId_key" ON "Client"("subId");
CREATE UNIQUE INDEX IF NOT EXISTS "Client_subToken_key" ON "Client"("subToken");
CREATE UNIQUE INDEX IF NOT EXISTS "Client_resellerId_username_key" ON "Client"("resellerId","username");
CREATE UNIQUE INDEX IF NOT EXISTS "ClientSetting_clientId_key" ON "ClientSetting"("clientId");
CREATE UNIQUE INDEX IF NOT EXISTS "Inbound_nodeId_tag_key" ON "Inbound"("nodeId","tag");
CREATE UNIQUE INDEX IF NOT EXISTS "PortShare_inboundId_tag_key" ON "PortShare"("inboundId","tag");
CREATE UNIQUE INDEX IF NOT EXISTS "ClientInbound_clientId_inboundId_key" ON "ClientInbound"("clientId","inboundId");
CREATE UNIQUE INDEX IF NOT EXISTS "TelegramUser_telegramId_key" ON "TelegramUser"("telegramId");
CREATE UNIQUE INDEX IF NOT EXISTS "SystemSetting_key_key" ON "SystemSetting"("key");
CREATE UNIQUE INDEX IF NOT EXISTS "Translation_locale_key_key" ON "Translation"("locale","key");
CREATE UNIQUE INDEX IF NOT EXISTS "ProtocolCredential_clientId_protocol_key" ON "ProtocolCredential"("clientId","protocol");

-- Indexes
DROP INDEX IF EXISTS "Client_subToken_idx";
CREATE INDEX IF NOT EXISTS "UserSession_userId_idx" ON "UserSession"("userId");
CREATE INDEX IF NOT EXISTS "UserSession_expiresAt_idx" ON "UserSession"("expiresAt");
CREATE INDEX IF NOT EXISTS "ApiKey_userId_idx" ON "ApiKey"("userId");
CREATE INDEX IF NOT EXISTS "Reseller_active_idx" ON "Reseller"("active");
CREATE INDEX IF NOT EXISTS "Plan_type_active_idx" ON "Plan"("type","active");
CREATE INDEX IF NOT EXISTS "Plan_sortOrder_idx" ON "Plan"("sortOrder");
CREATE INDEX IF NOT EXISTS "Client_expireAt_banned_idx" ON "Client"("expireAt","banned");
CREATE INDEX IF NOT EXISTS "Client_resellerId_idx" ON "Client"("resellerId");
CREATE INDEX IF NOT EXISTS "Client_nextTrafficResetAt_idx" ON "Client"("nextTrafficResetAt");
CREATE INDEX IF NOT EXISTS "Client_restrictionReason_idx" ON "Client"("restrictionReason");
CREATE INDEX IF NOT EXISTS "Client_resellerId_createdAt_idx" ON "Client"("resellerId","createdAt");
CREATE INDEX IF NOT EXISTS "Client_username_idx" ON "Client"("username");
CREATE INDEX IF NOT EXISTS "Client_email_idx" ON "Client"("email");
CREATE INDEX IF NOT EXISTS "Node_status_active_idx" ON "Node"("status","active");
CREATE INDEX IF NOT EXISTS "NodeMetric_nodeId_recordedAt_idx" ON "NodeMetric"("nodeId","recordedAt");
CREATE INDEX IF NOT EXISTS "NodeMetric_recordedAt_idx" ON "NodeMetric"("recordedAt");
CREATE INDEX IF NOT EXISTS "Inbound_nodeId_protocol_idx" ON "Inbound"("nodeId","protocol");
CREATE INDEX IF NOT EXISTS "PortShare_host_idx" ON "PortShare"("host");
CREATE INDEX IF NOT EXISTS "ClientInbound_inboundId_idx" ON "ClientInbound"("inboundId");
CREATE INDEX IF NOT EXISTS "RoutingRule_enabled_priority_idx" ON "RoutingRule"("enabled","priority");
CREATE INDEX IF NOT EXISTS "RoutingRule_nodeScope_idx" ON "RoutingRule"("nodeScope");
CREATE INDEX IF NOT EXISTS "TrafficLog_clientId_recordAt_idx" ON "TrafficLog"("clientId","recordAt");
CREATE INDEX IF NOT EXISTS "TrafficLog_nodeId_recordAt_idx" ON "TrafficLog"("nodeId","recordAt");
CREATE INDEX IF NOT EXISTS "TrafficLog_recordAt_idx" ON "TrafficLog"("recordAt");
CREATE INDEX IF NOT EXISTS "AuditLog_userId_createdAt_idx" ON "AuditLog"("userId","createdAt");
CREATE INDEX IF NOT EXISTS "AuditLog_action_resource_idx" ON "AuditLog"("action","resource");
CREATE INDEX IF NOT EXISTS "AuditLog_createdAt_idx" ON "AuditLog"("createdAt");
CREATE INDEX IF NOT EXISTS "Notification_targetUserId_read_idx" ON "Notification"("targetUserId","read");
CREATE INDEX IF NOT EXISTS "Notification_createdAt_idx" ON "Notification"("createdAt");
CREATE INDEX IF NOT EXISTS "BackupLog_createdAt_idx" ON "BackupLog"("createdAt");
CREATE INDEX IF NOT EXISTS "Translation_locale_idx" ON "Translation"("locale");
CREATE INDEX IF NOT EXISTS "TrafficReceipt_createdAt_idx" ON "TrafficReceipt"("createdAt");
CREATE INDEX IF NOT EXISTS "NodeCommand_nodeId_status_createdAt_idx" ON "NodeCommand"("nodeId","status","createdAt");

-- Foreign keys (idempotent)
CREATE OR REPLACE FUNCTION pg_temp.add_fk(tbl text, cname text, col text, reftbl text, ondel text) RETURNS void AS $f$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = cname AND conrelid = format('%I', tbl)::regclass) THEN
    EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I FOREIGN KEY (%I) REFERENCES %I("id") ON DELETE %s ON UPDATE CASCADE', tbl, cname, col, reftbl, ondel);
  END IF;
END $f$ LANGUAGE plpgsql;

SELECT pg_temp.add_fk('User','User_resellerId_fkey','resellerId','Reseller','SET NULL');
SELECT pg_temp.add_fk('UserSession','UserSession_userId_fkey','userId','User','CASCADE');
SELECT pg_temp.add_fk('ApiKey','ApiKey_userId_fkey','userId','User','CASCADE');
SELECT pg_temp.add_fk('Client','Client_resellerId_fkey','resellerId','Reseller','SET NULL');
SELECT pg_temp.add_fk('ClientSetting','ClientSetting_clientId_fkey','clientId','Client','CASCADE');
SELECT pg_temp.add_fk('NodeMetric','NodeMetric_nodeId_fkey','nodeId','Node','CASCADE');
SELECT pg_temp.add_fk('Inbound','Inbound_nodeId_fkey','nodeId','Node','CASCADE');
SELECT pg_temp.add_fk('PortShare','PortShare_inboundId_fkey','inboundId','Inbound','CASCADE');
SELECT pg_temp.add_fk('ClientInbound','ClientInbound_clientId_fkey','clientId','Client','CASCADE');
SELECT pg_temp.add_fk('ClientInbound','ClientInbound_inboundId_fkey','inboundId','Inbound','CASCADE');
SELECT pg_temp.add_fk('TrafficLog','TrafficLog_clientId_fkey','clientId','Client','CASCADE');
SELECT pg_temp.add_fk('TrafficLog','TrafficLog_nodeId_fkey','nodeId','Node','CASCADE');
SELECT pg_temp.add_fk('AuditLog','AuditLog_userId_fkey','userId','User','SET NULL');
SELECT pg_temp.add_fk('ProtocolCredential','ProtocolCredential_clientId_fkey','clientId','Client','CASCADE');
