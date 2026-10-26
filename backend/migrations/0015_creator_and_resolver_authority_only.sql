-- Resolution authority is creator or resolver, never the platform
-- administrator. ADR 0007 fixed the authority at series creation so the
-- platform could not resolve a creator's market; this removes the remaining
-- administrator path rather than only excluding it on creator-owned series.
--
-- No production data exists, so administrator-authority series are deleted
-- with their whole subtree instead of being migrated. They resolved through
-- the administrator evidence route, which this migration removes.
--
-- Instances with no series are untouched: those are the demo's own one-time
-- simulated markets, and the settlement worker records their deterministic
-- observation directly.
--
-- The purge flag opens the guarded delete path of the append-only triggers,
-- and deferring the circular instances/evidence constraint lets both tables
-- go in one transaction. Both revert at commit.

SET LOCAL polyntu.purge = 'on';
SET CONSTRAINTS ALL DEFERRED;

CREATE TEMP TABLE doomed ON COMMIT DROP AS
SELECT i.id, i.template_id FROM instances i
 WHERE i.series_id IN (SELECT id FROM market_series WHERE resolution_authority = 'admin');

DELETE FROM outbox            WHERE instance_id IN (SELECT id FROM doomed);
DELETE FROM positions         WHERE instance_id IN (SELECT id FROM doomed);
DELETE FROM trades            WHERE instance_id IN (SELECT id FROM doomed);
DELETE FROM settlement_claims WHERE instance_id IN (SELECT id FROM doomed);
DELETE FROM evidence          WHERE instance_id IN (SELECT id FROM doomed);
DELETE FROM instances         WHERE id IN (SELECT id FROM doomed);
-- A template row doubles as the browse grouping for a whole series, so only
-- delete one that no remaining instance still references.
DELETE FROM templates WHERE id IN (SELECT template_id FROM doomed)
  AND NOT EXISTS (SELECT 1 FROM instances i WHERE i.template_id = templates.id);

DELETE FROM market_series WHERE resolution_authority = 'admin';

-- Every series now states its authority at creation; there is no default.
ALTER TABLE market_series ALTER COLUMN resolution_authority DROP DEFAULT;
ALTER TABLE market_series DROP CONSTRAINT market_series_resolution_authority_check;
ALTER TABLE market_series ADD CHECK (resolution_authority IN ('creator','resolver'));
