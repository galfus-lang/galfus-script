#[cfg(test)]
mod tests;
pub const STD_IO_SOURCE: &str = include_str!("../rich_builtins/io.gfs");
pub const CONSTRAINTS_SOURCE: &str = include_str!("../rich_builtins/constraints.gfs");
pub const ITERABLE_SOURCE: &str = include_str!("../rich_builtins/iterable.gfs");

pub const TEXT_SOURCE: &str = include_str!("../rich_builtins/text.gfs");
pub const FORMAT_SOURCE: &str = include_str!("../rich_builtins/format.gfs");
pub const FORMAT_ANSI_SOURCE: &str = include_str!("../rich_builtins/format/ansi.gfs");
pub const THREAD_SOURCE: &str = include_str!("../rich_builtins/thread.gfs");

pub static BUILTIN_MODULES: &[(&str, &str)] = &[
    ("std/io", STD_IO_SOURCE),
    ("std/constraints", CONSTRAINTS_SOURCE),
    ("std/iterable", ITERABLE_SOURCE),
    ("std/thread", THREAD_SOURCE),
    ("text", TEXT_SOURCE),
    ("format", FORMAT_SOURCE),
    ("format/ansi", FORMAT_ANSI_SOURCE),
];

pub fn is_builtin_module(source: &str) -> bool {
    BUILTIN_MODULES.iter().any(|(name, _)| *name == source)
}
