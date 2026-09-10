SELECT count(*)
FROM instances i JOIN accounts a ON a.id=i.reserve_account_id
WHERE a.balance_micros < CASE
    WHEN i.result->>'kind'='winner' THEN coalesce((
        SELECT sum(p.quantity_millis)*1000 FROM positions p
        WHERE p.instance_id=i.id AND p.outcome_index=(i.result->>'outcome')::INTEGER
        AND NOT EXISTS (SELECT 1 FROM settlement_claims c WHERE c.instance_id=p.instance_id AND c.account_id=p.account_id)
    ),0)
    WHEN i.result->>'kind'='void' THEN coalesce((
        SELECT sum(x.credit) FROM (
            SELECT floor(sum(p.quantity_millis)*1000/cardinality(i.inventory)) AS credit
            FROM positions p WHERE p.instance_id=i.id
            AND NOT EXISTS (SELECT 1 FROM settlement_claims c WHERE c.instance_id=p.instance_id AND c.account_id=p.account_id)
            GROUP BY p.account_id
        ) x
    ),0)
    ELSE coalesce((
        SELECT max(x.liability) FROM (
            SELECT sum(p.quantity_millis)*1000 AS liability FROM positions p
            WHERE p.instance_id=i.id GROUP BY p.outcome_index
        ) x
    ),0)
END
