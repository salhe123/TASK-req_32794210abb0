ALTER TABLE package_variants
    ADD CONSTRAINT package_variants_inventory_item_id_fkey
    FOREIGN KEY (inventory_item_id) REFERENCES inventory_items(id);
ALTER TABLE package_variants
    ADD CONSTRAINT package_variants_time_slot_id_fkey
    FOREIGN KEY (time_slot_id) REFERENCES time_slots(id);
