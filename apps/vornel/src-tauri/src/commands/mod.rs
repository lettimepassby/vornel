pub mod clipboard;
pub mod fonts;
pub mod import;
pub mod keys;
pub mod records;
pub mod remote;
pub mod session;
pub mod sync;

pub type CmdResult<T> = Result<T, String>;

pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
