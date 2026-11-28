-- Add indexes for NodeCommand and Notification retention pruning
CREATE INDEX IF NOT EXISTS "NodeCommand_status_createdAt_idx" ON "NodeCommand" (status, "createdAt");
CREATE INDEX IF NOT EXISTS "Notification_createdAt_idx" ON "Notification" ("createdAt");
