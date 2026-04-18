-- Publishing a package is the gate that enforces cross-facility referential
-- integrity between a variant and its inventory_item / time_slot (see
-- handlers::packages::publish_package, which rejects any variant whose
-- inventory_item_id or time_slot_id is not in the package's own facility).
--
-- Keeping hard database-level FKs here would force variant creation to
-- pre-flight every linked ID, which the contract deliberately does NOT require
-- (a draft package is allowed to hold stale or cross-facility references so
-- the editor can fix them before publish). Drop the FKs so creation is
-- lenient; integrity is re-asserted at publish time.
ALTER TABLE package_variants DROP CONSTRAINT IF EXISTS package_variants_inventory_item_id_fkey;
ALTER TABLE package_variants DROP CONSTRAINT IF EXISTS package_variants_time_slot_id_fkey;
