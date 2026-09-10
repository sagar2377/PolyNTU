-- Initialized once by the application under the settings row lock. Kept out of
-- public views so future demo results cannot be derived from public instance IDs.
ALTER TABLE settings ADD COLUMN simulation_secret TEXT;
