#[cfg(all(
    tokio_unstable,
    feature = "io-uring",
    feature = "rt",
    feature = "fs",
    target_os = "linux"
))]
use crate::runtime::driver::op::Op;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub(crate) async fn create_dir_all_uring(path: &Path) -> io::Result<()> {
    if path == Path::new("") {
        return Ok(());
    }

    // First, check if the path exists.
    if mkdir_parent_missing(path).await? {
        return Ok(());
    }

    // Otherwise, we must create its parents.
    // For /a/b/c/d, we must first try /a/b/c, /a/b, /a, and /.
    // Hence, we can iterate over the positions of / in reverse,
    // finding the first / that appears after a directory that already exists.
    //
    // For example, suppose /a exists, but none of its children.
    // The creation of /a/b will be successful.
    // Hence, first_valid_separator_pos = 4.
    let mut first_valid_separator_pos = None;
    let path_bytes = path.as_os_str().as_bytes();
    for (separator_pos, _) in path_bytes
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, &byte)| byte == b'/')
    {
        let parent_bytes = &path_bytes[..separator_pos];
        let parent_str = OsStr::from_bytes(parent_bytes);
        let parent_path = Path::new(parent_str);
        if mkdir_parent_missing(parent_path).await? {
            first_valid_separator_pos = Some(separator_pos);
            break;
        }
    }
    let first_valid_separator_pos = first_valid_separator_pos.unwrap_or(0);

    // Once we have found the correct /,
    // we can iterate the remaining components in the forward direction.
    //
    // In the example /a/b/c/d path, there is only one remaining /, after c.
    // Hence, we first create /a/b/c.
    //
    // TODO: We're attempting to create all directories sequentially.
    // This would benefit from batching.
    for (separator_pos, _) in path_bytes
        .iter()
        .enumerate()
        .skip(first_valid_separator_pos + 1)
        .filter(|(_, &byte)| byte == b'/')
    {
        let parent_path = Path::new(OsStr::from_bytes(&path_bytes[..separator_pos]));
        mkdir_parent_created(parent_path).await?;
    }

    // We must finally create the last path (/a/b/c/d in our example).
    mkdir_parent_created(path).await?;
    Ok(())
}

async fn mkdir_parent_missing(path: &Path) -> io::Result<bool> {
    match Op::mkdir(path)?.await {
        Ok(()) => Ok(true),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        // TODO: replace with uring-based statx
        Err(_) if crate::fs::metadata(path).await?.is_dir() => Ok(true),
        Err(e) => Err(e),
    }
}

async fn mkdir_parent_created(path: &Path) -> io::Result<()> {
    match Op::mkdir(path)?.await {
        Ok(()) => Ok(()),
        // TODO: replace with uring-based statx
        Err(_) if crate::fs::metadata(path).await?.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}
