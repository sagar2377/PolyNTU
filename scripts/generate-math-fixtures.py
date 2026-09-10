"""Independent 80-digit Decimal reference; no backend dependencies required."""
from decimal import Decimal, localcontext, ROUND_CEILING, ROUND_FLOOR
from pathlib import Path
import json
import random

rng = random.Random(20260908)
cases = []
with localcontext() as ctx:
    ctx.prec = 80
    for n in (2, 3, 8):
        for liquidity in (10, 100, 1000, 100000):
            for _ in range(20):
                inventory = [rng.randrange(0, min(liquidity * 15000, 500000000)) for _ in range(n)]
                outcome = rng.randrange(n)
                side = rng.choice(("buy", "sell"))
                quantity = rng.choice((1, 17, 1000, 10000, 100000))
                if side == "sell":
                    quantity = min(quantity, inventory[outcome])
                if quantity == 0:
                    continue
                updated = inventory.copy()
                updated[outcome] += quantity if side == "buy" else -quantity
                if max(updated) - min(updated) > liquidity * 20000:
                    continue
                def cost(q):
                    b = Decimal(liquidity)
                    maximum = Decimal(max(q)) / 1000
                    return maximum + b * sum(((Decimal(x) / 1000 - maximum) / b).exp() for x in q).ln()
                delta = (cost(updated) - cost(inventory)) * 1000000
                amount = int(delta.to_integral_value(rounding=ROUND_CEILING)) if side == "buy" else int((-delta).to_integral_value(rounding=ROUND_FLOOR))
                cases.append(dict(inventory=inventory, liquidity=liquidity, outcome=outcome, side=side, quantity=quantity, amount_micros=amount))
    # Explicit high-inventory, concentration-edge and smallest-ledger-quantity
    # cases supplement the fixed random sample. Use the same independent C(q).
    for n in (2, 3, 8):
        for liquidity in (10, 100, 1000, 100000):
            maximum = 1000000000
            spread = min(liquidity * 20000, maximum)
            for inventory in ([maximum - 100000] * n, [maximum] + [maximum - spread] * (n - 1)):
                for outcome in (0, n - 1):
                    for side in ("buy", "sell"):
                        for quantity in (1, 100000):
                            updated = inventory.copy()
                            updated[outcome] += quantity if side == "buy" else -quantity
                            if min(updated) < 0 or max(updated) > maximum or max(updated) - min(updated) > liquidity * 20000:
                                continue
                            delta = (cost(updated) - cost(inventory)) * 1000000
                            amount = int(delta.to_integral_value(rounding=ROUND_CEILING)) if side == "buy" else int((-delta).to_integral_value(rounding=ROUND_FLOOR))
                            cases.append(dict(inventory=inventory, liquidity=liquidity, outcome=outcome, side=side, quantity=quantity, amount_micros=amount))
output = Path(__file__).resolve().parents[1] / "backend/tests/fixtures/lmsr-reference.json"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(cases, indent=2) + "\n", encoding="utf-8")
print(f"Generated {len(cases)} independent reference cases")
