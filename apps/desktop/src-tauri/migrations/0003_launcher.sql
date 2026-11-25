-- The app that manages an install and can update it: steam, galaxy, epic, heroic, legendary. NULL = none known.
ALTER TABLE installations ADD COLUMN launcher TEXT;
