"""NTU Omnibus API client (reverse-engineered from the app's OutSystems protocol).

All calls go to https://apps.ntu.edu.sg/NTUOmnibus/screenservices/... as JSON POSTs
with an anonymous session (no login needed). Verified working 2026-09-08.

Key protocol details:
  - viewName must be "MainFlow.Shuttle" (the host screen; actions live under
    CampusShuttle_MUI/MainFlow/RenderMap/... but run in the Shuttle screen context).
  - moduleVersion comes from GET /NTUOmnibus/moduleservices/moduleversioninfo.
  - X-CSRFToken header = the crf value from the nr2Users cookie.
  - DataActions need screenData.variables (RouteId, IsSBS_Route, CurrDateTime_Local
    in dd-MM-yyyy HH:mm:ss format) and clientVariables (SelectedRoute = route code).
  - Route codes: Red, Blue, Green, Brown, Grey (campus) / 179, 179A, 199 (public SBS).
"""
import time
from datetime import datetime, timedelta, timezone
from urllib.parse import unquote

import requests

HOST = "https://apps.ntu.edu.sg"
BASE = HOST + "/NTUOmnibus"

# Action path -> apiVersion key (from CampusShuttle_MUI scripts; verified live)
ACTIONS = {
    "token_and_keys":   ("CampusShuttle_MUI/ActionGetTokenAndKeys", "zy+7fhwsEHfAJ0U7rwwPZw", "action"),
    "active_buses":     ("CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetActiveBusServicesData",
                         "X18_TOlljR63ZC0gtWKItg", "data"),
    "pickup_checkpoints": ("CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetPickupCheckPoints",
                           "T2ld7BA3eca74ndBz+xz1Q", "data"),
    "routes_list":      ("CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetRoutesList",
                         "LTmSrAcALxEQn_uoCNK2RA", "data"),
    "eta_fms":          ("CampusShuttle_MUI/ActionGetETAAndNextETA_FMS",
                         "2vqnARteBS8UFZvhYCydcw", "action"),
    "eta_lta":          ("CampusShuttle_MUI/ActionGetETAAndNextETA_LTA",
                         "NBbTcPPSIJ15dc3NGRGWtA", "action"),
    "routes_minmax":    ("CampusShuttle_MUI/ActionGetRoutesMinMax",
                         "5h_9gx8v_PxQhWYJ6nSzaA", "action"),
    "announcements":    ("NTUOmnibus/ActionGetAnnoucements",
                         "agAUaSp+Gt+ysBfwCD5SRA", "action"),
}

ROUTE_COLORS = {
    "Red": "D71440", "Blue": "0054A6", "Green": "007C48",
    "Brown": "866D4B", "Grey": "808080",
    "179": "944496", "179A": "944496", "199": "944496", "default": "181C62",
}


def _colors_json():
    items = ",".join(
        f"{{'route_code':'{r}','color_code':'{c}','Order':'{i}'}}"
        for i, (r, c) in enumerate(ROUTE_COLORS.items(), start=1)
    )
    return "{'data':[" + items + "]}"


class NtuBusClient:
    def __init__(self):
        self.s = requests.Session()
        self.s.headers.update({
            "User-Agent": ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                           "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36"),
            "Accept": "application/json",
            "Content-Type": "application/json; charset=UTF-8",
            "Referer": BASE + "/",
            "Accept-Language": "en-US,en;q=0.9",
        })
        self.module_version = None
        self._bootstrap()

    def _bootstrap(self):
        """Get the current module version token + anonymous session cookies."""
        r = self.s.get(BASE + f"/moduleservices/moduleversioninfo?{int(time.time()*1000)}", timeout=15)
        self.module_version = r.json()["versionToken"]
        # First screenservices POST sets nr1Users/nr2Users cookies (anonymous session).

    def _csrf(self):
        for c in self.s.cookies:
            if c.name == "nr2Users":
                v = unquote(c.value or "")
                if "crf=" in v:
                    return v.split("crf=")[1].split(";")[0]
        return ""  # anonymous CSRF token is sent by the server on first call

    def _post(self, path, api_version, kind=None, inputs=None, screen_vars=None, client_vars=None):
        # `kind` ("action" vs "data") kept for call-site readability; both take the
        # same envelope — actions use inputParameters, data actions use screenData.
        payload = {
            "versionInfo": {"moduleVersion": self.module_version, "apiVersion": api_version},
            "viewName": "MainFlow.Shuttle",
        }
        if inputs is not None:
            payload["inputParameters"] = inputs
        if screen_vars is not None:
            payload["screenData"] = {"variables": screen_vars}
        if client_vars is not None:
            payload["clientVariables"] = client_vars
        r = self.s.post(BASE + "/screenservices/" + path, data=repr_json(payload),
                        headers={"X-CSRFToken": self._csrf(), "OutSystems-locale": "en-US"},
                        timeout=20)
        body = r.json()
        if body.get("exception"):
            raise RuntimeError(f"{path}: {body['exception'].get('message')}")
        return body.get("data", {})

    # ---------- public API ----------

    def get_config(self):
        """App config incl. FMS token, poll frequencies (ms), feature flags."""
        return self._post(*ACTIONS["token_and_keys"][:2], "action", inputs={})["Result"]

    def get_routes(self):
        """Campus route codes: Blue, Brown, Green, Grey, Red."""
        d = self._post(*ACTIONS["routes_list"][:2], "data",
                       screen_vars=self._screen_vars("Blue"),
                       client_vars=self._client_vars("Blue"))
        return [item["Route"] for item in d["RouteList"]["List"]]

    def get_active_buses(self, route="Blue"):
        """Live bus positions for a route.

        Returns list of {Vehplate, Lat, Lng, Speed, Direction, LoadInfo{CrowdLevel,...}}.
        """
        d = self._post(*ACTIONS["active_buses"][:2], "data",
                       screen_vars=self._screen_vars(route),
                       client_vars=self._client_vars(route))
        return d["Response"]["ActiveBusResult"]["Activebus"]["List"]

    def get_route_geometry(self, route="Blue"):
        """Route polyline checkpoints + pickup points (stops)."""
        d = self._post(*ACTIONS["pickup_checkpoints"][:2], "data",
                       screen_vars=self._screen_vars(route),
                       client_vars=self._client_vars(route))
        out = {"checkpoints": d["CheckPoint"]["CheckPointResult"]["CheckPoint"]["List"],
               "stops": d["PickupPoints"]["PickupPointResult"]["Pickuppoint"]["List"],
               "service_id": d.get("ServiceId")}
        return out

    def get_eta(self, bus_stop, route="Blue"):
        """Next two arrival times (minutes) for a campus (FMS) bus at a stop.

        bus_stop: stop name/code as shown in the app, e.g. "Hall 6", "Opp Hall 11".
        """
        d = self._post(*ACTIONS["eta_fms"][:2], "action",
                       inputs={"BusStopCode": bus_stop, "RouteCode": route})
        return {"eta_min": d["ETA"], "next_eta_min": d["NextETA"]}

    def get_eta_lta(self, bus_stop_code, service_no):
        """Arrival for public (SBS/LTA) buses, e.g. service 179 at a bus stop code."""
        d = self._post(*ACTIONS["eta_lta"][:2], "action",
                       inputs={"BusStopCode": bus_stop_code, "ServiceNo": service_no})
        return d

    def get_operating_hours(self, route="Blue"):
        """First/last bus times per route, day type and schedule type."""
        import json as _json
        d = self._post(*ACTIONS["routes_minmax"][:2], "action",
                       inputs={"RouteCode": route})
        rows = []
        for item in d["FMS_RouteMinMaxTime"]["List"]:
            inner = _json.loads(item["Json"])
            rows.extend(inner["RouteMinMaxTimeResult"]["RouteMinMaxTime"])
        return rows

    # ---------- helpers ----------

    @staticmethod
    def _screen_vars(route):
        now = (datetime.now(timezone.utc) + timedelta(hours=8)).strftime("%d-%m-%Y %H:%M:%S")
        return {
            "RouteId": route, "RouteName": route,
            "RouteColorCode": ROUTE_COLORS.get(route, "181C62"),
            "IsSBS_Route": route in ("179", "179A", "199"),
            "IsRouteDisabled": False,
            "CurrDateTime_Local": now,           # dd-MM-yyyy HH:mm:ss (SGT)
            "CheckIsScreenActive": True,
            "IsRenderMapActive": True,
            "UserCurrLat": "1.2804", "UserCurrLng": "103.8348",
        }

    @staticmethod
    def _client_vars(route):
        return {"SGHO_RouteColorCode": "", "SelectedRoute": route,
                "NTU_RouteColorCode": _colors_json()}


def repr_json(obj):
    """json.dumps with ensure_ascii=False, slash-free separators (like the app)."""
    import json
    return json.dumps(obj, ensure_ascii=False, separators=(", ", ": "))


if __name__ == "__main__":
    c = NtuBusClient()
    print("routes:", c.get_routes())
    for bus in c.get_active_buses("Blue"):
        print(f"  {bus['Vehplate']}: ({bus['Lat']}, {bus['Lng']}) "
              f"speed={bus['Speed']} heading={bus['Direction']} load={bus['LoadInfo']['CrowdLevel']}")
    print("ETA Hall 6 / Blue:", c.get_eta("Hall 6", "Blue"))
    hours = c.get_operating_hours("Blue")
    print("Blue hours:", hours[0] if hours else None)
