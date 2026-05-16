pub mod emit;
pub mod runtime;
pub mod tinycc;

pub const TCC_DIR: &str = env!("TCC_DIR");
pub const TCC_WIN32_DIR: &str = env!("TCC_WIN32_DIR");
pub const TCC_RUNTIME_DIR: &str = env!("TCC_RUNTIME_DIR");