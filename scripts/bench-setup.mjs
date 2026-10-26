// Shared setup for the benchmark workloads: accounts, markets, and creator
// signing.
//
// There is no administrator instance-creation route, so markets are published
// as series by a verified creator. Each series fixes its resolution authority
// at creation. The workloads use creator authority: the signing key is derived
// in-process from the creator's password exactly as the browser derives it, so
// the benchmark measures the platform's own verification path rather than the
// latency of an external resolver the project does not control.
import { randomUUID } from 'node:crypto';

const RESOLUTION_KEY_ITERATIONS = 600_000;
// DER prefix that wraps a raw 32-byte ed25519 seed as a PKCS#8 private key,
// matching frontend/src/api.js.
const ED25519_PKCS8_PREFIX = [
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65,
  0x70, 0x04, 0x22, 0x04, 0x20,
];

const toBase64 = (bytes) => Buffer.from(bytes).toString('base64');

/// The ed25519 seed ADR 0007 derives from the account password:
/// PBKDF2-HMAC-SHA256 over the password with the email-bound salt.
export async function resolutionKeySeed(email, password) {
  const encoder = new TextEncoder();
  const material = await crypto.subtle.importKey('raw', encoder.encode(password), 'PBKDF2', false, ['deriveBits']);
  const bits = await crypto.subtle.deriveBits({
    name: 'PBKDF2',
    salt: encoder.encode(`polyntu.resolution.v1:${email}`),
    iterations: RESOLUTION_KEY_ITERATIONS,
    hash: 'SHA-256',
  }, material, 256);
  return new Uint8Array(bits);
}

/// The creator's signing key: the published public key and a function that
/// signs one resolution statement.
export async function signingKey(email, password) {
  const seed = await resolutionKeySeed(email, password);
  const pkcs8 = new Uint8Array([...ED25519_PKCS8_PREFIX, ...seed]);
  const key = await crypto.subtle.importKey('pkcs8', pkcs8, { name: 'Ed25519' }, true, ['sign']);
  const jwk = await crypto.subtle.exportKey('jwk', key);
  let publicKey = jwk.x.replace(/-/g, '+').replace(/_/g, '/');
  while (publicKey.length % 4) publicKey += '=';
  const sign = async (instanceId, outcomeId, nonce) => toBase64(await crypto.subtle.sign(
    { name: 'Ed25519' },
    key,
    new TextEncoder().encode(`polyntu.resolution.v1:${instanceId}:${outcomeId}:${nonce}`),
  ));
  return { public_key: publicKey, sign };
}

export async function createAccounts(call, count, run, prefix = 'Benchmark') {
  const accounts = [];
  await parallel(Array.from({ length: count }), 5, async (_, i) => {
    accounts[i] = require200(
      await call('/admin/accounts', { display_name: `${prefix} ${run} ${i}` }, null, null, true),
      `create account ${i}`,
    );
  });
  return accounts;
}

/// Register one NTU account, have the administrator approve its creator
/// verification, and return its bearer session plus its signing key.
export async function verifiedCreator(call, run) {
  const email = `bench.${run}@ntu.edu.sg`;
  const password = 'benchmark password';
  const creator = require200(await call('/auth/register', {
    display_name: `Benchmark creator ${run}`,
    email,
    password,
  }), 'register the benchmark creator');
  await call('/verification-requests', {}, creator.token);
  const pending = require200(
    await call('/admin/verification-requests?status=pending', undefined, null, null, true),
    'list pending verification requests',
  );
  const request = pending.find((r) => r.account_id === creator.account.id);
  if (!request) throw new Error('the creator verification request was not listed');
  require200(
    await call(`/admin/verification-requests/${request.id}/decision`, { approve: true, reason: null }, null, null, true),
    'approve the benchmark creator',
  );
  return { ...creator, signing: await signingKey(email, password) };
}

/// Publish one-time weather series as `creator` and return their single
/// instances. `timing(i)` returns the close time and observation length for
/// market `i`.
export async function createMarkets(call, creator, count, run, { station, source, timing }) {
  const markets = [];
  await parallel(Array.from({ length: count }), 5, async (_, i) => {
    const { close, observationMs } = timing(i);
    const series = require200(await call('/series', {
      title: `Benchmark rainfall ${run} ${i}`,
      resolution_criterion: 'Yes when the benchmark station records at least 0.2 mm in the fixed observation window. Missing complete evidence voids the market.',
      rule: { kind: 'weather', station_id: station, threshold_milli_mm: 200 },
      source_id: source,
      liquidity_units: 100,
      resolution: { kind: 'creator', public_key: creator.signing.public_key },
      schedule: {
        kind: 'once',
        close_ms: close,
        observation_start_ms: close,
        observation_end_ms: close + observationMs,
        finalize_after_ms: close + observationMs + 1000,
        evidence_deadline_ms: close + observationMs + 120000,
      },
    }, creator.token), `publish benchmark market ${i}`);
    markets[i] = series.instances[0];
  });
  return markets;
}

/// Submit the creator's signed resolution for one instance.
export async function resolveMarket(call, creator, instanceId, outcomeId = 'yes') {
  const nonce = randomUUID();
  return call(`/instances/${instanceId}/resolution`, {
    outcome_id: outcomeId,
    nonce,
    signature: await creator.signing.sign(instanceId, outcomeId, nonce),
  }, creator.token);
}

export function require200(result, what) {
  if (result.status !== 200) throw new Error(`Setup failed to ${what}: ${result.status} ${JSON.stringify(result.value)}`);
  return result.value;
}

export async function parallel(items, count, action) {
  let index = 0;
  await Promise.all(Array.from({ length: count }, async () => {
    while (index < items.length) { const i = index++; await action(items[i], i); }
  }));
}

export { randomUUID };
