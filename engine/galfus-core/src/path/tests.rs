use super::ModulePath;

#[test]
fn new_accepts_a_simple_gfs_path() {
    let p = ModulePath::new("src/main.gfs").expect("valid path");
    assert_eq!(p.as_str(), "src/main.gfs");
}

#[test]
fn new_accepts_a_gfp_path() {
    let p = ModulePath::new("adapters/http.gfp").expect("valid path");
    assert_eq!(p.as_str(), "adapters/http.gfp");
}

#[test]
fn new_normalizes_backslashes_to_forward_slashes() {
    let p = ModulePath::new("src\\utils\\math.gfs").expect("valid path");
    assert_eq!(p.as_str(), "src/utils/math.gfs");
}

#[test]
fn new_resolves_single_dot_segments() {
    let p = ModulePath::new("src/./lib.gfs").expect("valid path");
    assert_eq!(p.as_str(), "src/lib.gfs");
}

#[test]
fn new_resolves_double_dot_segments() {
    let p = ModulePath::new("src/sub/../lib.gfs").expect("valid path");
    assert_eq!(p.as_str(), "src/lib.gfs");
}

#[test]
fn new_resolves_multiple_double_dot_segments() {
    let p = ModulePath::new("a/b/c/../../d.gfs").expect("valid path");
    assert_eq!(p.as_str(), "a/d.gfs");
}

#[test]
fn new_strips_leading_slashes() {
    let p = ModulePath::new("/src/main.gfs").expect("valid path");
    assert_eq!(p.as_str(), "src/main.gfs");
}

#[test]
fn new_rejects_null_bytes() {
    assert!(ModulePath::new("src/ma\0in.gfs").is_none());
}

#[test]
fn new_rejects_double_dot_escaping_root() {
    assert!(ModulePath::new("../escape.gfs").is_none());
}

#[test]
fn new_rejects_path_without_gfs_or_gfp_extension() {
    assert!(ModulePath::new("src/main.rs").is_none());
    assert!(ModulePath::new("src/main").is_none());
    assert!(ModulePath::new("src/main.GFS").is_none());
}

#[test]
fn new_preserves_original_casing_of_path_segments() {
    // Canonicalization must NOT alter case — paths are case-sensitive.
    let p = ModulePath::new("Src/MyModule.gfs").expect("valid path");
    assert_eq!(p.as_str(), "Src/MyModule.gfs");
}

#[test]
fn two_paths_with_same_canonical_form_are_equal() {
    let a = ModulePath::new("src/./sub/../lib.gfs").expect("valid path");
    let b = ModulePath::new("src/lib.gfs").expect("valid path");
    assert_eq!(a, b);
}

#[test]
fn display_uses_canonical_form() {
    let p = ModulePath::new("src\\./sub/.././lib.gfs").expect("valid path");
    assert_eq!(p.to_string(), "src/lib.gfs");
}
