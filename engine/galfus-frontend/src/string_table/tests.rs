use super::{NameId, StringTable};

#[test]
fn intern_returns_stable_id_for_the_same_string() {
    let mut table = StringTable::new();
    let id1 = table.intern("hello");
    let id2 = table.intern("hello");
    assert_eq!(id1, id2);
}

#[test]
fn intern_assigns_distinct_ids_for_distinct_strings() {
    let mut table = StringTable::new();
    let id_a = table.intern("alpha");
    let id_b = table.intern("beta");
    assert_ne!(id_a, id_b);
}

#[test]
fn resolve_returns_the_original_string() {
    let mut table = StringTable::new();
    let id = table.intern("galfus");
    assert_eq!(table.resolve(id), Some("galfus"));
}

#[test]
fn get_returns_none_for_unknown_string() {
    let table = StringTable::new();
    assert_eq!(table.get("missing"), None);
}

#[test]
fn get_returns_some_after_intern() {
    let mut table = StringTable::new();
    let id = table.intern("present");
    assert_eq!(table.get("present"), Some(id));
}

#[test]
fn name_id_from_one_table_is_not_meaningful_in_another_table() {
    // Two independent tables may assign the same numeric id to different strings.
    // This test verifies that the tables are fully independent and that a NameId
    // from one table is NOT valid in another — the raw ids must overlap for
    // independently interned names in the same position.
    let mut table_a = StringTable::new();
    let mut table_b = StringTable::new();

    // Both tables get their first intern at index 0.
    let id_from_a = table_a.intern("alpha");
    let id_from_b = table_b.intern("beta");

    // Both should have raw value 0 — the same slot in each independent table.
    assert_eq!(id_from_a.raw(), 0);
    assert_eq!(id_from_b.raw(), 0);

    // Resolving a's id in b's table returns "beta", NOT "alpha".
    // This is the invariant: NameId is ONLY valid inside its own table.
    assert_eq!(table_a.resolve(id_from_a), Some("alpha"));
    assert_eq!(table_b.resolve(NameId(id_from_a.raw())), Some("beta"));
}

#[test]
fn intern_is_case_sensitive() {
    let mut table = StringTable::new();
    let lower = table.intern("value");
    let upper = table.intern("Value");
    assert_ne!(lower, upper, "StringTable must not normalize casing");
    assert_eq!(table.resolve(lower), Some("value"));
    assert_eq!(table.resolve(upper), Some("Value"));
}
