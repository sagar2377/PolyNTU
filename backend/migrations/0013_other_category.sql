-- A market that fits none of the structured categories: the creator states the
-- resolution condition in prose. The category check on `templates` is the only
-- place that enumerates the values, so widening it is the whole schema change.
ALTER TABLE templates DROP CONSTRAINT templates_category_check;
ALTER TABLE templates ADD CHECK (category IN ('weather','bus','elections','queue_crowd','attendance','other'));
