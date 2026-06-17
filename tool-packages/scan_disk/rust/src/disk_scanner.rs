use crate::app_error::{AppError, AppResult};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScanRequest {
    pub path: String,
    pub max_depth: Option<usize>,
    pub max_children: Option<usize>,
    /// Soft deadline. Once exceeded, in-flight scan nodes return early with
    /// AppError::Cancelled — defaults to 60 s. Pass `None` (or 0) for the
    /// default.
    #[serde(default)]
    pub max_duration_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub children: Vec<DiskNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScanResult {
    pub root: DiskNode,
    pub scanned_entries: u64,
    pub elapsed_ms: u128,
    pub truncated: bool,
    pub scanned_at: String,
}

#[derive(Debug, Clone)]
struct ScanOptions {
    max_depth: usize,
    max_children: usize,
}

/// Shared scan state — read by every recursive worker via &.
struct ScanContext {
    options: ScanOptions,
    scanned_entries: AtomicU64,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    on_progress: Option<Box<dyn Fn(u64) + Send + Sync>>,
}

impl ScanContext {
    fn should_stop(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || Instant::now() >= self.deadline
    }

    fn note_entry(&self) {
        let prev = self.scanned_entries.fetch_add(1, Ordering::Relaxed);
        // Throttle progress callbacks to every 256 entries so an attached
        // listener doesn't get drowned during a tight loop.
        if let Some(cb) = &self.on_progress {
            let now = prev + 1;
            if now % 256 == 0 {
                cb(now);
            }
        }
    }
}

impl DiskScanRequest {
    fn options(&self) -> ScanOptions {
        ScanOptions {
            max_depth: self.max_depth.unwrap_or(4).clamp(1, 12),
            max_children: self.max_children.unwrap_or(12).clamp(1, 64),
        }
    }

    fn deadline(&self, started: Instant) -> Instant {
        let secs = self.max_duration_secs.unwrap_or(0);
        let secs = if secs == 0 { 60 } else { secs.min(600) };
        started + Duration::from_secs(secs)
    }
}

/// Cancellation handle returned by callers that want to abort an in-flight
/// scan. Cheap to clone; flipping it once stops every concurrent recursion.
#[derive(Debug, Clone, Default)]
pub struct ScanCancellation {
    flag: Arc<AtomicBool>,
}

impl ScanCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    pub fn handle(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }
}

pub fn scan_path(request: DiskScanRequest) -> AppResult<DiskScanResult> {
    scan_path_with(request, ScanCancellation::new(), None)
}

pub fn scan_path_with(
    request: DiskScanRequest,
    cancel: ScanCancellation,
    on_progress: Option<Box<dyn Fn(u64) + Send + Sync>>,
) -> AppResult<DiskScanResult> {
    let root = PathBuf::from(request.path.trim());
    if !root.exists() {
        return Err(AppError::InvalidInput(format!(
            "路径不存在: {}",
            root.display()
        )));
    }

    let started = Instant::now();
    let deadline = request.deadline(started);
    let options = request.options();
    let ctx = ScanContext {
        options,
        scanned_entries: AtomicU64::new(0),
        cancel: cancel.handle(),
        deadline,
        on_progress,
    };

    let (root, truncated) = scan_node(&root, 0, &ctx)?;

    Ok(DiskScanResult {
        root,
        scanned_entries: ctx.scanned_entries.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis(),
        truncated,
        scanned_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn scan_node(path: &Path, depth: usize, ctx: &ScanContext) -> AppResult<(DiskNode, bool)> {
    if ctx.should_stop() {
        return Err(AppError::Cancelled);
    }
    ctx.note_entry();
    let metadata = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string());

    if metadata.is_file() {
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: false,
                size_bytes: metadata.len(),
                file_count: 1,
                dir_count: 0,
                children: Vec::new(),
            },
            false,
        ));
    }

    if !metadata.is_dir() {
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: false,
                size_bytes: 0,
                file_count: 0,
                dir_count: 0,
                children: Vec::new(),
            },
            false,
        ));
    }

    if depth >= ctx.options.max_depth {
        let summary = summarize_dir(path, ctx);
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: true,
                size_bytes: summary.size_bytes,
                file_count: summary.file_count,
                dir_count: summary.dir_count,
                children: Vec::new(),
            },
            true,
        ));
    }

    let entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    let child_results = entries
        .par_iter()
        .filter_map(|entry| {
            if ctx.should_stop() {
                return None;
            }
            scan_node(entry, depth + 1, ctx).ok()
        })
        .collect::<Vec<_>>();

    if ctx.should_stop() {
        return Err(AppError::Cancelled);
    }

    let mut children = child_results
        .iter()
        .map(|(node, _)| node.clone())
        .collect::<Vec<_>>();
    let mut truncated = child_results.iter().any(|(_, was_truncated)| *was_truncated);

    let size_bytes = children.iter().map(|child| child.size_bytes).sum();
    let file_count = children.iter().map(|child| child.file_count).sum();
    let dir_count = 1 + children.iter().map(|child| child.dir_count).sum::<u64>();

    children.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    if children.len() > ctx.options.max_children {
        children.truncate(ctx.options.max_children);
        truncated = true;
    }

    Ok((
        DiskNode {
            name,
            path: path.display().to_string(),
            is_dir: true,
            size_bytes,
            file_count,
            dir_count,
            children,
        },
        truncated,
    ))
}

#[derive(Default)]
struct DirSummary {
    size_bytes: u64,
    file_count: u64,
    dir_count: u64,
}

fn summarize_dir(path: &Path, ctx: &ScanContext) -> DirSummary {
    let mut summary = DirSummary::default();
    for entry in WalkDir::new(path).follow_links(false).into_iter().filter_map(Result::ok) {
        if ctx.should_stop() {
            break;
        }
        ctx.note_entry();
        match entry.metadata() {
            Ok(metadata) if metadata.is_file() => {
                summary.file_count += 1;
                summary.size_bytes = summary.size_bytes.saturating_add(metadata.len());
            }
            Ok(metadata) if metadata.is_dir() => {
                summary.dir_count += 1;
            }
            _ => {}
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use std::fs;

    /// Build a small temp tree:
    ///   root/
    ///     a.bin   (10 bytes)
    ///     sub/
    ///       b.bin (20 bytes)
    ///       c.bin (30 bytes)
    fn build_tree() -> TempDir {
        let dir = TempDir::new("scan");
        fs::write(dir.path().join("a.bin"), vec![0u8; 10]).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.bin"), vec![0u8; 20]).unwrap();
        fs::write(sub.join("c.bin"), vec![0u8; 30]).unwrap();
        dir
    }

    fn request(path: &Path, depth: Option<usize>, children: Option<usize>) -> DiskScanRequest {
        DiskScanRequest {
            path: path.display().to_string(),
            max_depth: depth,
            max_children: children,
            max_duration_secs: None,
        }
    }

    #[test]
    fn scan_reports_aggregated_size_and_counts() {
        let dir = build_tree();
        let result = scan_path(request(dir.path(), Some(4), Some(12))).unwrap();
        assert!(result.root.is_dir);
        assert_eq!(result.root.size_bytes, 60, "10 + 20 + 30 = 60");
        assert_eq!(result.root.file_count, 3);
        // root + sub
        assert_eq!(result.root.dir_count, 2);
    }

    #[test]
    fn scan_sorts_children_by_size_descending() {
        let dir = build_tree();
        let result = scan_path(request(dir.path(), Some(4), Some(12))).unwrap();
        let names: Vec<_> = result
            .root
            .children
            .iter()
            .map(|c| c.name.clone())
            .collect();
        // sub (50 bytes) should come before a.bin (10 bytes).
        let sub_idx = names.iter().position(|n| n == "sub").unwrap();
        let a_idx = names.iter().position(|n| n == "a.bin").unwrap();
        assert!(sub_idx < a_idx, "expected larger-first ordering, got {names:?}");
    }

    #[test]
    fn max_depth_truncates_subtree_but_keeps_summary() {
        let dir = build_tree();
        // depth=1 means root itself is depth 0; children are at depth 1 -> hit
        // the depth cap. The 'sub' dir should expose size/file totals but no
        // children, and `truncated` should be true.
        let result = scan_path(request(dir.path(), Some(1), Some(12))).unwrap();
        let sub = result
            .root
            .children
            .iter()
            .find(|c| c.name == "sub")
            .expect("sub must be present");
        assert!(sub.is_dir);
        assert!(sub.children.is_empty(), "subtree must be empty at depth cap");
        assert_eq!(sub.size_bytes, 50);
        assert_eq!(sub.file_count, 2);
        assert!(result.truncated);
    }

    #[test]
    fn max_children_truncates_largest_subset() {
        let dir = TempDir::new("many");
        for (idx, size) in [10u64, 50, 30, 100, 20].iter().enumerate() {
            fs::write(dir.path().join(format!("f{idx}.bin")), vec![0u8; *size as usize]).unwrap();
        }
        let result = scan_path(request(dir.path(), Some(3), Some(2))).unwrap();
        assert_eq!(result.root.children.len(), 2);
        // After sort, the two largest (100 and 50) survive.
        let sizes: Vec<_> = result
            .root
            .children
            .iter()
            .map(|c| c.size_bytes)
            .collect();
        assert_eq!(sizes, vec![100, 50]);
        assert!(result.truncated);
    }

    #[test]
    fn missing_path_errors() {
        let request = DiskScanRequest {
            path: "Z:/no/such/path/exists/here/iPet/test".to_string(),
            max_depth: None,
            max_children: None,
            max_duration_secs: None,
        };
        let err = scan_path(request).expect_err("missing path must error");
        assert!(matches!(err, AppError::InvalidInput(_)), "got {err:?}");
    }

    #[test]
    fn options_clamp_to_safe_bounds() {
        let request = DiskScanRequest {
            path: ".".to_string(),
            max_depth: Some(9999),
            max_children: Some(9999),
            max_duration_secs: None,
        };
        let opts = request.options();
        assert_eq!(opts.max_depth, 12);
        assert_eq!(opts.max_children, 64);

        let request = DiskScanRequest {
            path: ".".to_string(),
            max_depth: Some(0),
            max_children: Some(0),
            max_duration_secs: None,
        };
        let opts = request.options();
        assert_eq!(opts.max_depth, 1);
        assert_eq!(opts.max_children, 1);
    }

    #[test]
    fn cancellation_aborts_scan() {
        let dir = build_tree();
        let cancel = ScanCancellation::new();
        cancel.cancel(); // pre-cancelled — first call into scan_node bails
        let err = scan_path_with(request(dir.path(), Some(4), Some(12)), cancel, None)
            .expect_err("pre-cancelled scan must error");
        assert!(matches!(err, AppError::Cancelled), "got {err:?}");
    }

    #[test]
    fn progress_callback_fires_for_large_trees() {
        let dir = TempDir::new("prog");
        // Need at least 256 entries to trigger the throttled callback.
        for i in 0..300 {
            fs::write(dir.path().join(format!("f{i}.bin")), b"x").unwrap();
        }
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let probe = seen.clone();
        let cb: Box<dyn Fn(u64) + Send + Sync> = Box::new(move |n| {
            probe.store(n, std::sync::atomic::Ordering::Relaxed);
        });
        scan_path_with(request(dir.path(), Some(2), Some(400)), ScanCancellation::new(), Some(cb))
            .unwrap();
        assert!(
            seen.load(std::sync::atomic::Ordering::Relaxed) >= 256,
            "progress callback should have fired"
        );
    }
}
