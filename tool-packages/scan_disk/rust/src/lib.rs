pub mod app_error;
pub mod disk_scanner;

#[cfg(test)]
mod testutil;

pub use app_error::{AppError, AppResult};
pub use disk_scanner::{
    scan_path, scan_path_with, DiskNode, DiskScanRequest, DiskScanResult, ScanCancellation,
};

pub fn run_tool(
    path: impl Into<String>,
    max_depth: Option<usize>,
    max_children: Option<usize>,
) -> AppResult<String> {
    let result = scan_path(DiskScanRequest {
        path: path.into(),
        max_depth,
        max_children,
        max_duration_secs: None,
    })?;
    Ok(serde_json::to_string(&result)?)
}
