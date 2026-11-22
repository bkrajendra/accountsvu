//! HSN/SAC directory (Phase 2.5.1): status, search, and syncing from the
//! GSTN master.
//!
//! The download runs in Rust from one fixed URL (`GSTN_MASTER_URL`); the web
//! view never supplies a URL or a path. The bytes go to the application core,
//! which checks the workbook and rebuilds the local directory.

use std::time::Duration;

use av_application::hsn_directory::{
    DirectorySearchInput, DirectorySearchResult, DirectoryStatusView, DirectorySyncResult,
    GSTN_MASTER_URL, MAX_MASTER_BYTES,
};
use av_application::{AppCore, Permission, RequestContext};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::CommandError;

type CmdResult<T> = Result<T, CommandError>;

fn ctx() -> RequestContext {
    RequestContext::local_owner()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    /// `downloading`, then `updating`.
    stage: &'static str,
    received: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn hsn_directory_status(core: State<'_, AppCore>) -> CmdResult<DirectoryStatusView> {
    Ok(core.hsn_directory_status(&ctx()).await?)
}

#[tauri::command]
pub async fn search_hsn_directory(
    core: State<'_, AppCore>,
    input: DirectorySearchInput,
) -> CmdResult<DirectorySearchResult> {
    Ok(core.search_hsn_directory(&ctx(), &input).await?)
}

/// Download the GSTN master and rebuild the directory from it.
#[tauri::command]
pub async fn sync_hsn_directory(
    core: State<'_, AppCore>,
    on_progress: Channel<SyncProgress>,
) -> CmdResult<DirectorySyncResult> {
    // Check permission before going to the network.
    ctx().require(Permission::ManageBusiness)?;
    let bytes = download(&on_progress).await?;
    let _ = on_progress.send(SyncProgress {
        stage: "updating",
        received: bytes.len() as u64,
        total: Some(bytes.len() as u64),
    });
    Ok(core
        .import_hsn_master(&ctx(), &bytes, "gstn", GSTN_MASTER_URL)
        .await?)
}

/// Rebuild the directory from a copy of the master picked by the owner
/// (for machines without internet access). `None` when cancelled.
#[tauri::command]
pub async fn import_hsn_master_file(
    app: AppHandle,
    core: State<'_, AppCore>,
) -> CmdResult<Option<DirectorySyncResult>> {
    ctx().require(Permission::ManageBusiness)?;
    let Some(path) = crate::host::open_dialog(&app, ("Excel workbook", "xlsx")).await else {
        return Ok(None);
    };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size > MAX_MASTER_BYTES as u64 {
        return Err(CommandError::new(
            "invalid_input",
            "The file is larger than 20 MB; it isn't the HSN/SAC master.",
        ));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| CommandError::new("io_error", format!("The file could not be read: {e}")))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(Some(
        core.import_hsn_master(&ctx(), &bytes, "file", &name)
            .await?,
    ))
}

fn network_error(e: impl std::fmt::Display) -> CommandError {
    tracing::warn!(error = %e, "HSN/SAC master download failed");
    CommandError::new(
        "network_error",
        "Couldn't download the HSN/SAC master from the GST portal. Check the internet connection and try again, or import a downloaded copy.",
    )
}

async fn download(progress: &Channel<SyncProgress>) -> CmdResult<Vec<u8>> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("AccountsVU/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(network_error)?;
    let mut resp = client
        .get(GSTN_MASTER_URL)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    let total = resp.content_length();
    let too_big = || {
        CommandError::new(
            "invalid_input",
            "The GST portal sent a file larger than 20 MB; it isn't the HSN/SAC master.",
        )
    };
    if total.is_some_and(|t| t > MAX_MASTER_BYTES as u64) {
        return Err(too_big());
    }
    let mut bytes =
        Vec::with_capacity(total.unwrap_or(1 << 20).min(MAX_MASTER_BYTES as u64) as usize);
    let mut last = 0usize;
    while let Some(chunk) = resp.chunk().await.map_err(network_error)? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_MASTER_BYTES {
            return Err(too_big());
        }
        // Report every 64 KB or so.
        if bytes.len() - last >= 64 * 1024 {
            last = bytes.len();
            let _ = progress.send(SyncProgress {
                stage: "downloading",
                received: bytes.len() as u64,
                total,
            });
        }
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(CommandError::new(
            "invalid_input",
            "The GST portal didn't return an Excel workbook. Try again later, or import a downloaded copy.",
        ));
    }
    Ok(bytes)
}
