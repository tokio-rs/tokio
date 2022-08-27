//! Filesystem manipulation operations.

mod directory;
pub use directory::remove_dir;

mod file;
pub use file::remove_file;
pub use file::rename;
pub use file::File;

mod open_options;
pub use open_options::OpenOptions;
