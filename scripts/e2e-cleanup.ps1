# Purge the markets, series, and verification requests that local runs of
# scripts/browser-e2e.mjs leave in this demo database. CI runs the suite
# against its own throwaway database; locally it shares this one, so run this
# after a local E2E pass to keep the demo clean. The e2e.* accounts and their
# ledger history stay behind: transfers are append-only and the accounts are
# not visible in the UI.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'configure-dev.ps1')
$psql = Join-Path $projectRoot '.tools\postgres\pgsql\bin\psql.exe'
$sql = @'
BEGIN;
SET LOCAL polyntu.purge = 'on';
SET CONSTRAINTS ALL DEFERRED;
DO $guard$
BEGIN
  IF EXISTS (
    SELECT 1 FROM instances i
    JOIN accounts r ON r.id = i.reserve_account_id
    JOIN market_series s ON s.id = i.series_id
    WHERE s.title LIKE 'E2E %' AND r.balance_micros <> 0
  ) THEN
    RAISE EXCEPTION 'an E2E instance still holds reserve units; refusing to purge';
  END IF;
END
$guard$;
DELETE FROM outbox WHERE instance_id IN (SELECT i.id FROM instances i JOIN market_series s ON s.id = i.series_id WHERE s.title LIKE 'E2E %');
DELETE FROM positions WHERE instance_id IN (SELECT i.id FROM instances i JOIN market_series s ON s.id = i.series_id WHERE s.title LIKE 'E2E %');
DELETE FROM trades WHERE instance_id IN (SELECT i.id FROM instances i JOIN market_series s ON s.id = i.series_id WHERE s.title LIKE 'E2E %');
DELETE FROM settlement_claims WHERE instance_id IN (SELECT i.id FROM instances i JOIN market_series s ON s.id = i.series_id WHERE s.title LIKE 'E2E %');
DELETE FROM evidence WHERE instance_id IN (SELECT i.id FROM instances i JOIN market_series s ON s.id = i.series_id WHERE s.title LIKE 'E2E %');
DELETE FROM instances WHERE series_id IN (SELECT id FROM market_series WHERE title LIKE 'E2E %');
DELETE FROM templates WHERE id IN (SELECT id FROM market_series WHERE title LIKE 'E2E %');
DELETE FROM market_series WHERE title LIKE 'E2E %';
DELETE FROM verification_requests WHERE account_id IN (SELECT id FROM accounts WHERE email LIKE 'e2e.%');
COMMIT;
'@
& $psql $env:DATABASE_URL -v ON_ERROR_STOP=1 -c $sql
