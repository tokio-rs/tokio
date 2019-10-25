use crate::fs::asyncify;

use std::fs::Permissions;
use std::io;
use std::path::Path;

/// Changes the permissions found on a file or a directory.
///
/// This is an async version of [`std::fs::set_permissions`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.set_permissions.html
pub async fn set_permissions(path: impl AsRef<Path>, perm: Permissions) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    asyncify(|| std::fs::set_permissions(path, perm)).await
}
