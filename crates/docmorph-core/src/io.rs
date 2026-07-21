#[cfg(test)]
use std::sync::Arc;
use std::{
    fmt,
    io::Read,
    os::fd::OwnedFd,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use docmorph_contracts::Diagnostic;
use rustix::fs::{AtFlags, Mode, OFlags, fsync, linkat, open, openat, unlinkat};
use sha2::{Digest, Sha256};

#[cfg(not(unix))]
compile_error!("docmorph-core local I/O policy requires Unix descriptor-relative filesystem APIs");

/// Maximum local input size accepted by the Phase 0 policy.
pub const MAX_INPUT_BYTES: u64 = 200 * 1024 * 1024;
const STAGING_RETRY_LIMIT: u64 = 16;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Canonical filesystem roots from which inputs may be accepted.
#[derive(Clone)]
pub struct InputPolicy {
    allowed_roots: Vec<PathBuf>,
    #[cfg(test)]
    before_bounded_read: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    before_path_walk: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl fmt::Debug for InputPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("InputPolicy");
        debug.field("allowed_roots", &self.allowed_roots);
        #[cfg(test)]
        debug.field("before_bounded_read", &self.before_bounded_read.is_some());
        #[cfg(test)]
        debug.field("before_path_walk", &self.before_path_walk.is_some());
        debug.finish()
    }
}

impl InputPolicy {
    #[must_use]
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self {
            allowed_roots,
            #[cfg(test)]
            before_bounded_read: None,
            #[cfg(test)]
            before_path_walk: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    fn with_before_bounded_read<F>(mut self, hook: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.before_bounded_read = Some(Arc::new(hook));
        self
    }

    #[cfg(test)]
    #[must_use]
    fn with_before_path_walk<F>(mut self, hook: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.before_path_walk = Some(Arc::new(hook));
        self
    }

    fn canonical_roots(&self) -> Vec<PathBuf> {
        self.allowed_roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .collect()
    }
}

/// A canonical regular-file input that was read through the bounded policy handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedInput {
    path: PathBuf,
    bytes: Vec<u8>,
}

/// Verifiable metadata for a complete destination artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Publication {
    pub byte_len: u64,
    pub sha256: String,
}

/// A destination parent directory held open across staging and publication.
pub struct PinnedDestination {
    parent: OwnedFd,
    filename: std::ffi::OsString,
    #[cfg(test)]
    force_post_link_sync_failure: bool,
    #[cfg(test)]
    after_sync_before_link: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ValidatedInput {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl PinnedDestination {
    #[cfg(test)]
    #[must_use]
    fn with_forced_post_link_sync_failure(mut self) -> Self {
        self.force_post_link_sync_failure = true;
        self
    }

    #[cfg(test)]
    #[must_use]
    fn with_after_sync_before_link<F>(mut self, hook: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.after_sync_before_link = Some(Arc::new(hook));
        self
    }

    /// Stages and links an artifact relative to the already-open destination parent.
    pub fn publish_no_overwrite(self, bytes: &[u8]) -> Result<Publication, Diagnostic> {
        let (staging, staging_fd) = create_owned_staging(&self.parent, &self.filename)?;
        let mut staging_file = std::fs::File::from(staging_fd);
        use std::io::Write;
        let result = (|| {
            staging_file.write_all(bytes).map_err(|_| Diagnostic {
                code: "output_staging_failed".into(),
                message: "staging bytes cannot be written".into(),
            })?;
            staging_file.flush().map_err(|_| Diagnostic {
                code: "output_staging_failed".into(),
                message: "staging bytes cannot be flushed".into(),
            })?;
            staging_file.sync_all().map_err(|_| Diagnostic {
                code: "output_staging_failed".into(),
                message: "staging bytes cannot be synced".into(),
            })?;
            #[cfg(test)]
            if let Some(hook) = &self.after_sync_before_link {
                hook();
            }
            linkat(
                &self.parent,
                &staging,
                &self.parent,
                &self.filename,
                AtFlags::empty(),
            )
            .map_err(|error| Diagnostic {
                code: if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "output_exists"
                } else {
                    "output_publication_failed"
                }
                .into(),
                message: error.to_string(),
            })?;
            #[cfg(test)]
            let synced = if self.force_post_link_sync_failure {
                Err(rustix::io::Errno::IO)
            } else {
                fsync(&self.parent)
            };
            #[cfg(not(test))]
            let synced = fsync(&self.parent);
            synced.map_err(|_| Diagnostic {
                code: "output_published_durability_unknown".into(),
                message: "destination entry was created but the destination directory sync \
                          failed; the published file is kept"
                    .into(),
            })?;
            Ok::<_, Diagnostic>(())
        })();
        let _ = unlinkat(&self.parent, &staging, AtFlags::empty());
        result?;
        let sha256 = format!("{:x}", Sha256::digest(bytes));
        Ok(Publication {
            byte_len: bytes.len() as u64,
            sha256,
        })
    }
}

fn create_owned_staging(
    parent: &OwnedFd,
    filename: &std::ffi::OsStr,
) -> Result<(std::ffi::OsString, OwnedFd), Diagnostic> {
    for _ in 0..STAGING_RETRY_LIMIT {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging = std::ffi::OsString::from(format!(
            ".{}.{}.{}.staging",
            filename.to_string_lossy(),
            std::process::id(),
            sequence
        ));
        match openat(
            parent,
            &staging,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(file) => return Ok((staging, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(output_staging_failed(&format!(
                    "staging file cannot be created: {error}"
                )));
            }
        }
    }
    Err(output_staging_failed(
        "staging file name collision retry limit reached",
    ))
}

/// Canonicalizes, bounds, and reads a regular local input without adapter involvement.
pub fn validate_input(
    policy: &InputPolicy,
    input: impl AsRef<Path>,
) -> Result<ValidatedInput, Diagnostic> {
    let input = input.as_ref().canonicalize().map_err(|_| Diagnostic {
        code: "input_missing".into(),
        message: "input path does not exist or cannot be resolved".into(),
    })?;
    let (root, relative) = matching_root(policy, &input, "input_outside_allowed_root")?;
    let root_fd = open_directory(&root, "input_missing")?;
    #[cfg(test)]
    if let Some(hook) = &policy.before_path_walk {
        hook();
    }
    let file_fd = open_relative_file(&root_fd, &relative).map_err(|_| Diagnostic {
        code: "input_missing".into(),
        message: "input cannot be opened safely".into(),
    })?;
    let mut file = std::fs::File::from(file_fd);
    let metadata = file.metadata().map_err(|_| Diagnostic {
        code: "input_missing".into(),
        message: "input metadata cannot be read".into(),
    })?;
    if !metadata.is_file() {
        return Err(Diagnostic {
            code: "input_not_regular_file".into(),
            message: "input must be a regular file".into(),
        });
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(Diagnostic {
            code: "input_too_large".into(),
            message: "input exceeds the 200 MB limit".into(),
        });
    }

    #[cfg(test)]
    if let Some(hook) = &policy.before_bounded_read {
        hook();
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Diagnostic {
            code: "input_read_failed".into(),
            message: "input cannot be read".into(),
        })?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(Diagnostic {
            code: "input_too_large".into(),
            message: "input grew beyond the 200 MB limit while being read".into(),
        });
    }

    Ok(ValidatedInput {
        path: root.join(relative),
        bytes,
    })
}

/// Resolves a destination beneath an allowed canonical root before adapter execution.
pub fn validate_destination(
    policy: &InputPolicy,
    destination: impl AsRef<Path>,
) -> Result<PinnedDestination, Diagnostic> {
    let destination = destination.as_ref();
    let filename = destination.file_name().ok_or_else(|| Diagnostic {
        code: "output_invalid_destination".into(),
        message: "destination must name a file".into(),
    })?;
    let parent = destination.parent().ok_or_else(|| Diagnostic {
        code: "output_invalid_destination".into(),
        message: "destination must have a parent directory".into(),
    })?;
    let parent = parent.canonicalize().map_err(|_| Diagnostic {
        code: "output_invalid_destination".into(),
        message: "destination parent does not exist or cannot be resolved".into(),
    })?;
    let (root, relative) = matching_root(policy, &parent, "output_outside_allowed_root")?;
    let root_fd = open_directory(&root, "output_invalid_destination")?;
    let parent = open_relative_directory(&root_fd, &relative).map_err(|_| Diagnostic {
        code: "output_invalid_destination".into(),
        message: "destination parent cannot be opened safely".into(),
    })?;
    Ok(PinnedDestination {
        parent,
        filename: filename.to_os_string(),
        #[cfg(test)]
        force_post_link_sync_failure: false,
        #[cfg(test)]
        after_sync_before_link: None,
    })
}

fn output_staging_failed(message: &str) -> Diagnostic {
    Diagnostic {
        code: "output_staging_failed".into(),
        message: message.into(),
    }
}

fn matching_root(
    policy: &InputPolicy,
    path: &Path,
    code: &str,
) -> Result<(PathBuf, PathBuf), Diagnostic> {
    policy
        .canonical_roots()
        .into_iter()
        .find_map(|root| {
            path.strip_prefix(&root)
                .ok()
                .map(|relative| (root, relative.to_path_buf()))
        })
        .ok_or_else(|| Diagnostic {
            code: code.into(),
            message: "path is outside configured allowed roots".into(),
        })
}

fn open_directory(path: &Path, code: &str) -> Result<OwnedFd, Diagnostic> {
    open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Diagnostic {
        code: code.into(),
        message: "directory cannot be opened safely".into(),
    })
}

fn open_relative_directory(
    parent: &OwnedFd,
    relative: &Path,
) -> Result<OwnedFd, rustix::io::Errno> {
    let mut directory = duplicate_fd(parent)?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(rustix::io::Errno::INVAL);
        };
        directory = openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
    }
    Ok(directory)
}

fn open_relative_file(parent: &OwnedFd, relative: &Path) -> Result<OwnedFd, rustix::io::Errno> {
    let mut components = relative.components().peekable();
    let mut directory = duplicate_fd(parent)?;
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(rustix::io::Errno::INVAL);
        };
        if components.peek().is_some() {
            directory = openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
        } else {
            return openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            );
        }
    }
    Ok(directory)
}

fn duplicate_fd(fd: &OwnedFd) -> Result<OwnedFd, rustix::io::Errno> {
    rustix::io::fcntl_dupfd_cloexec(fd, 0)
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
        sync::{Arc, Barrier, mpsc},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    use docmorph_contracts::{
        ContractVersion, ExecutionBounds, Operation, OperationKind, Provenance,
    };

    use super::{InputPolicy, MAX_INPUT_BYTES, STAGING_SEQUENCE, validate_destination};
    use crate::{Lifecycle, MockAdapter};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(std::path::PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_nanos();
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("docmorph-core-growth-race-{unique}-{sequence}"));
            std::fs::create_dir(&path).expect("temporary root is created");
            Self(path)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn operation() -> Operation {
        Operation {
            contract_version: ContractVersion::CURRENT,
            kind: OperationKind::MockTransform,
            bounds: ExecutionBounds::default(),
            provenance: Provenance {
                request_id: "growth-race".into(),
                source: "io-policy-test".into(),
            },
        }
    }

    #[test]
    fn pinned_parent_descriptor_keeps_cloexec_when_parent_is_the_allowed_root() {
        let root = TempRoot::new();
        let policy = InputPolicy::new(vec![root.0.clone()]);

        let pinned = validate_destination(&policy, root.0.join("result.mock"))
            .expect("destination is pinned at the allowed root");

        let flags = rustix::io::fcntl_getfd(&pinned.parent).expect("descriptor flags are read");
        assert!(
            flags.contains(rustix::io::FdFlags::CLOEXEC),
            "pinned parent descriptor must carry FD_CLOEXEC"
        );
    }

    #[test]
    fn reports_unknown_durability_when_only_the_post_link_directory_sync_fails() {
        let root = TempRoot::new();
        let destination = root.0.join("result.mock");
        let pinned = validate_destination(&InputPolicy::new(vec![root.0.clone()]), &destination)
            .expect("destination is pinned beneath the allowed root")
            .with_forced_post_link_sync_failure();

        let error = pinned
            .publish_no_overwrite(b"durable bytes")
            .expect_err("a post-link directory sync failure is reported");

        assert_eq!(error.code, "output_published_durability_unknown");
        assert_eq!(std::fs::read(&destination).unwrap(), b"durable bytes");
    }

    #[test]
    fn concurrent_publications_wait_for_two_synced_owned_stages_before_linking() {
        let root = TempRoot::new();
        let destination = root.0.join("result.mock");
        let policy = InputPolicy::new(vec![root.0.clone()]);
        let barrier = Arc::new(Barrier::new(3));
        let (synced, reached_sync) = mpsc::channel();
        let collision_sequence = STAGING_SEQUENCE.load(Ordering::Relaxed);
        let unowned_stage = root.0.join(format!(
            ".result.mock.{}.{}.staging",
            std::process::id(),
            collision_sequence
        ));
        std::fs::write(&unowned_stage, b"live bytes").expect("unowned stage is created");

        let first = {
            let barrier = Arc::clone(&barrier);
            let synced = synced.clone();
            let policy = policy.clone();
            let destination = destination.clone();
            thread::spawn(move || {
                validate_destination(&policy, destination)
                    .expect("first destination is pinned")
                    .with_after_sync_before_link(move || {
                        synced.send(()).expect("sync arrival is recorded");
                        barrier.wait();
                    })
                    .publish_no_overwrite(b"first complete candidate")
            })
        };
        let second = {
            let barrier = Arc::clone(&barrier);
            let policy = policy.clone();
            let destination = destination.clone();
            thread::spawn(move || {
                validate_destination(&policy, destination)
                    .expect("second destination is pinned")
                    .with_after_sync_before_link(move || {
                        synced.send(()).expect("sync arrival is recorded");
                        barrier.wait();
                    })
                    .publish_no_overwrite(b"second complete candidate")
            })
        };

        reached_sync
            .recv()
            .expect("first synced stage reaches the hook");
        reached_sync
            .recv()
            .expect("second synced stage reaches the hook");
        let owned_stages: Vec<_> = std::fs::read_dir(&root.0)
            .expect("publication directory is readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path() != unowned_stage
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(&format!(".result.mock.{}.", std::process::id()))
            })
            .collect();
        assert_eq!(
            owned_stages.len(),
            2,
            "two distinct owned stages are synced"
        );
        barrier.wait();

        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .filter(|error| error.code == "output_exists")
                .count(),
            1
        );
        let final_bytes = std::fs::read(&destination).expect("winner is published");
        assert!(
            final_bytes == b"first complete candidate"
                || final_bytes == b"second complete candidate"
        );
        assert_eq!(
            std::fs::read(&unowned_stage).unwrap(),
            b"live bytes",
            "unowned staging files are not deleted"
        );
        assert!(
            std::fs::read_dir(&root.0)
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| {
                    entry.path() == unowned_stage
                        || !entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(&format!(".result.mock.{}.", std::process::id()))
                })
        );
    }

    #[test]
    fn rejects_a_file_that_grows_after_the_metadata_gate_before_mock_invocation() {
        let root = TempRoot::new();
        let input = root.0.join("growth-race.bin");
        let file = std::fs::File::create(&input).expect("input file is created");
        file.set_len(MAX_INPUT_BYTES)
            .expect("input starts exactly at the policy limit");

        let growth_target = input.clone();
        let policy = InputPolicy::new(vec![root.0.clone()]).with_before_bounded_read(move || {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&growth_target)
                .expect("growth-race input is reopened");
            file.write_all(&[0]).expect("one byte is appended");
        });
        let mock = Arc::new(MockAdapter::default());
        let lifecycle = Lifecycle::new(policy, Arc::clone(&mock));

        let error = lifecycle
            .submit(&operation(), &input, root.0.join("output.mock"))
            .expect_err("an input that grows during the bounded read is rejected");

        assert_eq!(error.code, "input_too_large");
        assert_eq!(mock.invocation_count(), 0);
    }

    #[test]
    fn rejects_a_path_component_replaced_with_an_escape_symlink_before_mock_invocation() {
        let root = TempRoot::new();
        let outside = TempRoot::new();
        let input_directory = root.0.join("input");
        std::fs::create_dir(&input_directory).expect("input directory is created");
        let input = input_directory.join("document.mock");
        std::fs::write(&input, b"approved bytes").expect("approved input is written");
        std::fs::write(outside.0.join("document.mock"), b"attacker bytes")
            .expect("outside input is written");

        let moved_directory = root.0.join("input-held");
        let replacement = input_directory.clone();
        let escape_target = outside.0.clone();
        let policy = InputPolicy::new(vec![root.0.clone()]).with_before_path_walk(move || {
            std::fs::rename(&replacement, &moved_directory).expect("input directory is pinned");
            std::os::unix::fs::symlink(&escape_target, &replacement)
                .expect("escape symlink replaces the path component");
        });
        let mock = Arc::new(MockAdapter::default());
        let lifecycle = Lifecycle::new(policy, Arc::clone(&mock));
        let destination = root.0.join("output.mock");

        let error = lifecycle
            .submit(&operation(), &input, &destination)
            .expect_err("descriptor-relative traversal rejects the replacement symlink");

        assert_eq!(error.code, "input_missing");
        assert!(!destination.exists());
        assert_eq!(mock.invocation_count(), 0);
    }
}
