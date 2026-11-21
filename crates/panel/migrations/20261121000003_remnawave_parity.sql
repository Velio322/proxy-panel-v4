-- Custom Xray profile and host header fields
ALTER TABLE "Host"
  ADD COLUMN IF NOT EXISTS "tags" TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
  ADD COLUMN IF NOT EXISTS "allowInsecure" BOOLEAN,
  ADD COLUMN IF NOT EXISTS "pinnedPeerCertSha256" TEXT,
  ADD COLUMN IF NOT EXISTS "verifyPeerCertByName" TEXT,
  ADD COLUMN IF NOT EXISTS "mihomoIpVersion" TEXT,
  ADD COLUMN IF NOT EXISTS "serverDescription" TEXT;

ALTER TABLE "Node"
  ADD COLUMN IF NOT EXISTS "xrayProfile" JSONB;
