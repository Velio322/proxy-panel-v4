-- Store installed NaiveProxy / Mieru versions and CPU architecture on Node
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "naiveVersion" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "mieruVersion" TEXT;
ALTER TABLE "Node" ADD COLUMN IF NOT EXISTS "arch" TEXT NOT NULL DEFAULT 'amd64';
