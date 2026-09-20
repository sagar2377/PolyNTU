"""The NTU Bus API resolver provider.

A standalone service that implements PolyNTU's external-resolver contract
against the live NTU Omnibus API (the campus bus app's backend, wrapped by
ntubus_client.py). The platform's bus adapter
(POST /api/v2/resolvers/ntu-bus) asks this service first and only falls back
to the deterministic simulated feed when this service stays without a
definitive answer.

How it answers: a background thread polls the live positions of every
watched route and each route's pickup points, and records an arrival event
whenever a bus comes within NTUBUS_RADIUS_M of a stop. A resolver request
for the window [start, end) is answered Yes when any arrival was recorded
inside the window, No when the poller watched the route through the whole
window and no bus came, and pending when it cannot know (no coverage, the
window is in the future, or the route or stop is unknown). When the
underlying Omnibus API itself stays unreachable the service answers 503 so
the platform falls back rather than recording a false No.

Contract (what the platform POSTs here, JSON):
  {"instance_id": "...", "series_id": "...", "bracket_start_ms": 0,
   "window_start_ms": 0, "window_end_ms": 0,
   "outcomes": [{"id": "yes", "label": "Yes"}, {"id": "no", "label": "No"}],
   "title": "...", "evidence_deadline_ms": 0,
   "rule": {"kind": "bus", "route_id": "NTU-blue", "direction": "clockwise",
            "stop_id": "opp-spms"}}
The answer is exactly one of {"outcome_id": "<published id>"} or
{"pending": true} (with an extra human-readable "reason").

Run: python provider/ntubus/service.py   (needs the `requests` package)
Configuration via environment variables, see the README.
"""
import json
import math
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from ntubus_client import NtuBusClient

POLL_SECONDS = float(os.environ.get("NTUBUS_POLL_SECONDS", "12"))
RADIUS_M = float(os.environ.get("NTUBUS_RADIUS_M", "60"))
BIND = os.environ.get("NTUBUS_BIND", "127.0.0.1:8090")
ROUTES = [r.strip() for r in os.environ.get("NTUBUS_ROUTES", "Blue,Red,Green,Brown,Grey").split(",") if r.strip()]
BUFFER_MS = int(float(os.environ.get("NTUBUS_BUFFER_MINUTES", "120")) * 60_000)
GEOMETRY_TTL_MS = int(float(os.environ.get("NTUBUS_GEOMETRY_MINUTES", "10")) * 60_000)
CLIENT_ATTEMPTS = int(os.environ.get("NTUBUS_CLIENT_ATTEMPTS", "3"))

EARTH_RADIUS_M = 6371000.0


def now_ms():
    return int(time.time() * 1000)


def normalize(name):
    """'Opp Hall 11' and 'opp-hall-11' are the same stop."""
    return " ".join((name or "").strip().lower().replace("-", " ").split())


def distance_m(lat1, lng1, lat2, lng2):
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lng2 - lng1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * EARTH_RADIUS_M * math.asin(math.sqrt(a))


class Watcher:
    """Records arrival events and poll coverage per route."""

    def __init__(self):
        self.lock = threading.Lock()
        # Constructed lazily on the poller thread: the Omnibus bootstrap is a
        # live network call and must not delay startup or the module import.
        self.client = None
        # route -> {(stop_key, stop_name, lat, lng)} refreshed every TTL
        self.stops = {}
        self.stops_fetched_at = {}
        # (route, stop_key) -> [timestamps of arrival events]
        self.events = {}
        # route -> [timestamps of successful polls]
        self.polls = {route: [] for route in ROUTES}
        self.last_api_success = 0.0
        self.last_error = None

    def _call(self, method, *args):
        """One client API call with retries; raises the last error."""
        if self.client is None:
            self.client = NtuBusClient()
        last = None
        for _ in range(max(1, CLIENT_ATTEMPTS)):
            try:
                return getattr(self.client, method)(*args)
            except Exception as e:  # the app's protocol errors and HTTP both raise
                last = e
                time.sleep(1.0)
        raise last or RuntimeError("client call failed without an error")

    def _refresh_stops(self, route):
        if now_ms() - self.stops_fetched_at.get(route, 0) < GEOMETRY_TTL_MS:
            return
        geometry = self._call("get_route_geometry", route)
        stops = set()
        for s in geometry.get("stops", []):
            try:
                stops.add((normalize(s.get("Pickupname")), s.get("Pickupname"),
                           float(s.get("Lat")), float(s.get("Lng"))))
            except (TypeError, ValueError):
                continue
        with self.lock:
            self.stops[route] = stops
            self.stops_fetched_at[route] = now_ms()

    def poll_once(self):
        for route in ROUTES:
            try:
                self._refresh_stops(route)
                buses = self._call("get_active_buses", route)
                with self.lock:
                    stops = self.stops.get(route, set())
                    stamp = now_ms()
                    self.polls.setdefault(route, []).append(stamp)
                    for bus in buses:
                        try:
                            blat, blng = float(bus["Lat"]), float(bus["Lng"])
                        except (TypeError, ValueError, KeyError):
                            continue
                        for stop_key, _name, slat, slng in stops:
                            if distance_m(blat, blng, slat, slng) <= RADIUS_M:
                                self.events.setdefault((route, stop_key), []).append(stamp)
                    self.last_api_success = time.time()
                    self.last_error = None
            except Exception as e:
                with self.lock:
                    self.last_error = f"{route}: {e}"
        self.prune()

    def prune(self):
        cutoff = now_ms() - BUFFER_MS
        with self.lock:
            self.events = {k: [t for t in v if t >= cutoff] for k, v in self.events.items()}
            for route in list(self.polls):
                self.polls[route] = [t for t in self.polls[route] if t >= cutoff]

    def healthy(self):
        # No lock: reading one float is atomic, and status() calls this
        # while already holding the lock (a plain Lock is not reentrant).
        return time.time() - self.last_api_success < 3 * POLL_SECONDS

    def status(self):
        with self.lock:
            return {
                "ok": self.healthy(),
                "routes": {r: {"stops": len(self.stops.get(r, ())), "polls": len(self.polls.get(r, []))}
                           for r in ROUTES},
                "events": sum(len(v) for v in self.events.values()),
                "last_error": self.last_error,
            }

    def answer(self, request):
        """Answer one resolver request; returns the JSON-able response."""
        rule = request.get("rule") or {}
        route_id = str(rule.get("route_id") or "")
        stop_id = str(rule.get("stop_id") or "")
        window_start = int(request.get("window_start_ms") or 0)
        window_end = int(request.get("window_end_ms") or 0)
        outcomes = request.get("outcomes") or []
        route = None
        for watched in ROUTES:
            if route_id.lower().removeprefix("ntu-").removeprefix("sbs-") == watched.lower():
                route = watched
                break
        if route is None:
            return {"pending": True, "reason": f"route {route_id!r} is not watched"}
        stop_key = normalize(stop_id)
        with self.lock:
            known = [s for s in self.stops.get(route, ()) if s[0] == stop_key]
            if not known:
                return {"pending": True, "reason": f"stop {stop_id!r} is not on route {route}"}
            if window_start > now_ms():
                return {"pending": True, "reason": "the window has not started yet"}
            polls = sorted(self.polls.get(route, []))
            before = [t for t in polls if t <= window_start]
            if not before:
                return {"pending": True, "reason": "no poll coverage before the window"}
            in_window = [t for t in polls if window_start <= t <= window_end]
            if not in_window:
                return {"pending": True, "reason": "no polls inside the window"}
            gap = max(b - a for a, b in zip([before[-1]] + in_window, in_window))
            if gap > 2.5 * POLL_SECONDS * 1000:
                return {"pending": True, "reason": "a poll gap leaves the window partially uncovered"}
            arrivals = [t for t in self.events.get((route, stop_key), []) if window_start <= t < window_end]
        yes = next((o for o in outcomes if str(o.get("id", "")).lower() == "yes"
                    or str(o.get("label", "")).lower() == "yes"), None)
        if yes is None or len(outcomes) != 2:
            return {"pending": True, "reason": "cannot map the published outcomes to yes/no"}
        no = next(o for o in outcomes if o is not yes)
        return {"outcome_id": yes["id"] if arrivals else no["id"]}


WATCHER = Watcher()


def run_poller():
    while True:
        started = time.time()
        try:
            WATCHER.poll_once()
        except Exception as e:  # never let the thread die
            WATCHER.last_error = str(e)
        time.sleep(max(1.0, POLL_SECONDS - (time.time() - started)))


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, body):
        payload = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        self._send(200, WATCHER.status())

    def do_POST(self):
        try:
            length = int(self.headers.get("content-length") or 0)
            request = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self._send(400, {"error": "the body must be JSON"})
            return
        if not WATCHER.healthy():
            self._send(503, {"error": "the Omnibus API is unreachable; retrying"})
            return
        self._send(200, WATCHER.answer(request))

    def log_message(self, format, *args):  # quieter logs; errors stay visible
        pass


def main():
    # Bind and print before the poller starts: the service must answer (with
    # 503 while unhealthy) even while the first Omnibus poll cycle is still
    # running or the API is unreachable.
    host, _, port = BIND.rpartition(":")
    server = ThreadingHTTPServer((host or "127.0.0.1", int(port or 8090)), Handler)
    threading.Thread(target=run_poller, daemon=True).start()
    print(f"NTU Bus API resolver provider listening on {BIND}, watching {ROUTES}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
