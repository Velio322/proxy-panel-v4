-- Telegram WEB proxy (telemt) protocol and its node core version column.
ALTER TYPE "Protocol" ADD VALUE IF NOT EXISTS 'TGWEB';
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "telemtVersion" TEXT;
