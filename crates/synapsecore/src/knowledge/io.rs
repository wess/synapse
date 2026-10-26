use anyhow::{Result, ensure};
#[cfg(not(unix))]
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Read;
#[cfg(unix)]
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub(super) struct Folder {
    pub path: PathBuf,
    #[cfg(any(unix, windows))]
    handle: File,
    #[cfg(windows)]
    ancestors: Vec<File>,
}

impl Folder {
    pub fn open(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        let handle = {
            use std::os::unix::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)?
        };
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(path)?;
            let metadata = file.metadata()?;
            ensure!(
                metadata.is_dir() && !reparse(&metadata),
                "expected a real directory"
            );
            file
        };
        Ok(Self {
            path: path.to_owned(),
            #[cfg(any(unix, windows))]
            handle,
            #[cfg(windows)]
            ancestors: Vec::new(),
        })
    }

    pub fn child(&self, name: &str, create: bool) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let name = std::ffi::CString::new(name)?;
            if create {
                let result =
                    unsafe { libc::mkdirat(self.handle.as_raw_fd(), name.as_ptr(), 0o700) };
                if result != 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            let handle = self.openat(&name, libc::O_RDONLY | libc::O_DIRECTORY, false)?;
            Ok(Self {
                path: self.path.join(name.to_str()?),
                handle,
            })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            if create {
                match fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_dir() && !reparse(&metadata),
                "expected a real directory"
            );
            let folder = Self::open(&path)?;
            #[cfg(windows)]
            let folder = {
                let mut folder = folder;
                folder.ancestors = self
                    .ancestors
                    .iter()
                    .map(File::try_clone)
                    .collect::<std::io::Result<_>>()?;
                folder.ancestors.push(self.handle.try_clone()?);
                folder
            };
            Ok(folder)
        }
    }

    #[cfg(unix)]
    fn openat(&self, name: &std::ffi::CStr, flags: i32, regular: bool) -> Result<File> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let fd = unsafe {
            libc::openat(
                self.handle.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        ensure!(
            !regular || file.metadata()?.is_file(),
            "expected a regular file"
        );
        Ok(file)
    }

    pub fn file(&self, name: &str, create: bool) -> Result<File> {
        #[cfg(unix)]
        {
            let name = std::ffi::CString::new(name)?;
            self.openat(
                &name,
                if create {
                    libc::O_RDWR | libc::O_CREAT
                } else {
                    libc::O_RDONLY
                },
                true,
            )
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => ensure!(
                    metadata.is_file() && !reparse(&metadata),
                    "expected a regular file"
                ),
                Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let mut options = OpenOptions::new();
            options.read(true).write(create).create(create);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
            }
            let file = options.open(path)?;
            let metadata = file.metadata()?;
            ensure!(
                metadata.is_file() && !reparse(&metadata),
                "expected a regular file"
            );
            Ok(file)
        }
    }

    pub fn source(&self, relative: &str) -> Result<File> {
        let mut parts = Path::new(relative).components().peekable();
        let mut folder = None;
        while let Some(part) = parts.next() {
            let Component::Normal(part) = part else {
                anyhow::bail!("invalid source path")
            };
            let part = part
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid source path"))?;
            let parent = folder.as_ref().unwrap_or(self);
            if parts.peek().is_none() {
                return parent.file(part, false);
            }
            folder = Some(parent.child(part, false)?);
        }
        anyhow::bail!("source path is empty")
    }

    pub fn entries(&self, limit: usize) -> Result<(Vec<std::ffi::OsString>, bool)> {
        #[cfg(unix)]
        {
            use std::os::fd::IntoRawFd;
            use std::os::unix::ffi::OsStrExt;
            let fd = self
                .openat(c".", libc::O_RDONLY | libc::O_DIRECTORY, false)?
                .into_raw_fd();
            let directory = unsafe { libc::fdopendir(fd) };
            if directory.is_null() {
                let error = std::io::Error::last_os_error();
                unsafe {
                    libc::close(fd);
                }
                return Err(error.into());
            }
            let mut names = Vec::new();
            let truncated = loop {
                let entry = unsafe { libc::readdir(directory) };
                if entry.is_null() {
                    break false;
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                if names.len() == limit {
                    break true;
                }
                names.push(std::ffi::OsStr::from_bytes(name).to_owned());
            };
            unsafe {
                libc::closedir(directory);
            }
            Ok((names, truncated))
        }
        #[cfg(not(unix))]
        {
            let mut names = Vec::new();
            for entry in fs::read_dir(&self.path)? {
                if names.len() == limit {
                    return Ok((names, true));
                }
                names.push(entry?.file_name());
            }
            Ok((names, false))
        }
    }

    pub fn bytes(&self, name: &str, limit: usize) -> Result<Vec<u8>> {
        let file = self.file(name, false)?;
        ensure!(
            file.metadata()?.len() <= limit as u64,
            "file exceeds size limit"
        );
        let mut bytes = Vec::new();
        file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= limit, "file exceeds size limit");
        Ok(bytes)
    }

    pub fn replace(&self, name: &str, content: &[u8]) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let temporary = std::ffi::CString::new(format!(
                ".write{}{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))?;
            let mut file = self.openat(
                &temporary,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                true,
            )?;
            let result = (|| -> Result<()> {
                file.write_all(content)?;
                file.sync_all()?;
                let target = std::ffi::CString::new(name)?;
                let result = unsafe {
                    libc::renameat(
                        self.handle.as_raw_fd(),
                        temporary.as_ptr(),
                        self.handle.as_raw_fd(),
                        target.as_ptr(),
                    )
                };
                if result != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                self.handle.sync_all()?;
                Ok(())
            })();
            if result.is_err() {
                unsafe {
                    libc::unlinkat(self.handle.as_raw_fd(), temporary.as_ptr(), 0);
                }
            }
            result
        }
        #[cfg(not(unix))]
        {
            crate::files::atomicreplace(&self.path.join(name), content, None)
        }
    }
}

#[cfg(not(unix))]
fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) fn knowledge(root: &Folder, create: bool) -> Result<Folder> {
    root.child(".synapse", create)?.child("knowledge", create)
}
