-- Installed DLC as the launcher reports it: JSON array of {"id", "name"}. Replaced on every scan.
ALTER TABLE installations ADD COLUMN dlc TEXT;
