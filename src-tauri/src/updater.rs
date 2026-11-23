//! In-app updates. The app asks the GitHub release feed (`latest.json`) for a
//! newer version; installers are signed with the project's updater key and
//! the signature is checked before anything is installed.
//!
//! Everything runs in Rust: the web view only sees the version details and
//! download progress, and never chooses a URL or a file.

use std::sync::Mutex;

use av_application::AppCore;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::commands::CommandError;

type CmdResult<T> = Result<T, CommandError>;

/// Where people can download installers by hand.
const RELEASES_PAGE: &str = "https://github.com/bkrajendra/accountsvu/releases/latest";
const SUPPORT_PAGE: &str = "https://buymeacoffee.com/aszy3yb";

/// The update found by the last check, kept for `install_update`.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl PendingUpdate {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Update>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    version: String,
    /// RFC 3339 publish time from the feed, when given.
    date: Option<String>,
    notes: Option<String>,
    /// False where this install can't replace itself (Linux .deb/.rpm);
    /// the UI offers the download page instead.
    can_install: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateProgress {
    Started { content_length: Option<u64> },
    Progress { downloaded: u64 },
    Finished,
}

fn can_self_install() -> bool {
    // On Linux only the AppImage can update itself.
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

fn check_err(e: tauri_plugin_updater::Error) -> CommandError {
    tracing::warn!(error = %e, "update check failed");
    CommandError::new(
        "update_unavailable",
        "Couldn't check for updates. Check your internet connection and try again.",
    )
}

fn install_err(e: tauri_plugin_updater::Error) -> CommandError {
    tracing::warn!(error = %e, "update install failed");
    CommandError::new(
        "update_failed",
        format!(
            "The update couldn't be installed ({e}). You can download it from the releases page instead."
        ),
    )
}

/// Asks the release feed whether a newer version exists.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<UpdateInfo>> {
    let found = app
        .updater()
        .map_err(check_err)?
        .check()
        .await
        .map_err(check_err)?;
    let info = found.as_ref().map(|u| UpdateInfo {
        current_version: u.current_version.clone(),
        version: u.version.clone(),
        date: u
            .raw_json
            .get("pub_date")
            .and_then(|v| v.as_str())
            .map(String::from),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        can_install: can_self_install(),
    });
    *pending.slot() = found;
    Ok(info)
}

/// Downloads the update found by the last check, verifies its signature,
/// installs it and restarts. On Windows the installer closes the app itself.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    core: State<'_, AppCore>,
    on_progress: Channel<UpdateProgress>,
) -> CmdResult<()> {
    let update = pending
        .slot()
        .take()
        .ok_or_else(|| CommandError::new("not_found", "Check for updates first."))?;

    // Progress is sent every 256 KiB so a large download doesn't flood IPC.
    const STEP: u64 = 256 * 1024;
    let mut started = false;
    let mut downloaded: u64 = 0;
    let mut reported: u64 = 0;
    let bytes = update
        .download(
            |chunk, total| {
                if !started {
                    started = true;
                    let _ = on_progress.send(UpdateProgress::Started {
                        content_length: total,
                    });
                }
                downloaded = downloaded.saturating_add(chunk as u64);
                if downloaded - reported >= STEP || Some(downloaded) == total {
                    reported = downloaded;
                    let _ = on_progress.send(UpdateProgress::Progress { downloaded });
                }
            },
            || {
                let _ = on_progress.send(UpdateProgress::Finished);
            },
        )
        .await
        .map_err(install_err)?;

    update.install(bytes).map_err(install_err)?;
    core.close().await;
    app.restart();
}

/// Opens the Buy Me a Coffee page in the default browser.
#[tauri::command]
pub fn open_support_page(app: AppHandle) -> CmdResult<()> {
    app.opener()
        .open_url(SUPPORT_PAGE, None::<&str>)
        .map_err(|e| CommandError::new("io_error", e.to_string()))
}

/// Opens the releases page in the default browser.
#[tauri::command]
pub fn open_releases_page(app: AppHandle) -> CmdResult<()> {
    app.opener()
        .open_url(RELEASES_PAGE, None::<&str>)
        .map_err(|e| CommandError::new("io_error", e.to_string()))
}
