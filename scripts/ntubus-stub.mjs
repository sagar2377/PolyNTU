// A deterministic stand-in for provider/ntubus/service.py, for CI and for
// local runs without the real Omnibus client.
//
// It implements the same resolver contract the platform's bus adapter speaks:
// POST a bracket request, get back exactly one published outcome id, or
// {"pending": true} before the observation window has ended. Answers are
// derived from the instance id, so a given bracket always resolves the same
// way and a failure is reproducible.
//
// Run: node scripts/ntubus-stub.mjs [port]
// The platform points POLYNTU_NTUBUS_PROVIDER at it, exactly as it would at
// the real provider; the adapter then tags the evidence "ntubus-live", because
// from its point of view a provider answered.
import { createServer } from "node:http";
import { createHash } from "node:crypto";

const port = Number(process.argv[2] ?? process.env.NTUBUS_STUB_PORT ?? 8090);
const started = Date.now();

function answer(request) {
  const outcomes = Array.isArray(request.outcomes) ? request.outcomes : [];
  if (outcomes.length !== 2) {
    return { pending: true, reason: "the stub only answers binary brackets" };
  }
  // Pending until the bracket's own observation window has ended, so the
  // platform never sees a result before it is due, exactly like the real
  // provider.
  const windowEnd = Number(request.window_end_ms ?? 0);
  if (!windowEnd || Date.now() < windowEnd) {
    return { pending: true, reason: "the observation window has not ended" };
  }
  const digest = createHash("sha256").update(String(request.instance_id ?? "")).digest();
  const yes = outcomes.find((o) => String(o.id).toLowerCase() === "yes") ?? outcomes[0];
  const no = outcomes.find((o) => o !== yes) ?? outcomes[1];
  return { outcome_id: digest[0] % 2 === 0 ? yes.id : no.id };
}

const server = createServer((req, res) => {
  const send = (status, body) => {
    const payload = JSON.stringify(body);
    res.writeHead(status, { "content-type": "application/json", "content-length": Buffer.byteLength(payload) });
    res.end(payload);
  };
  if (req.method === "GET") {
    return send(200, { ok: true, stub: true, uptime_ms: Date.now() - started });
  }
  if (req.method !== "POST") return send(405, { error: "POST a resolver request" });
  let body = "";
  req.on("data", (chunk) => {
    body += chunk;
    // The platform caps resolver responses at 64 KiB; refusing an oversized
    // request here keeps the stub from buffering without bound.
    if (body.length > 65536) req.destroy();
  });
  req.on("end", () => {
    try {
      send(200, answer(JSON.parse(body || "{}")));
    } catch {
      send(400, { error: "the body must be JSON" });
    }
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`ntubus stub provider listening on 127.0.0.1:${port}`);
});
