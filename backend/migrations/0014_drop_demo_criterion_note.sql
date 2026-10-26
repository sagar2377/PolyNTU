-- The demo instances seeded by earlier builds appended a note to every
-- resolution criterion: "Demo: deterministic simulated evidence. Missing final
-- evidence voids shares at 1/n units each." The UI already conveys both the
-- simulated source and the void policy, so the note is noise on every market.
-- New instances no longer add it; this removes it from the rows that carry it.
--
-- resolution_criterion is immutable under protect_instance, so the trigger is
-- disabled for exactly this statement and re-enabled immediately. Version and
-- lifecycle fields are untouched, and no other column changes.
ALTER TABLE instances DISABLE TRIGGER protect_instance;
UPDATE instances
   SET resolution_criterion = btrim(replace(resolution_criterion,
       ' Demo: deterministic simulated evidence. Missing final evidence voids shares at 1/n units each.', ''))
 WHERE resolution_criterion LIKE '%Demo: deterministic simulated evidence%';
ALTER TABLE instances ENABLE TRIGGER protect_instance;
