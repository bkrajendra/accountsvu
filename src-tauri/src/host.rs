//! Desktop-only commands (Phase 1.7): the app lock, encrypted backups and
//! restore, and saving/opening printed documents.
//!
//! File locations are always chosen in native dialogs opened from Rust, so the
//! web view never supplies a path to read or write.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use av_application::backup::{BackupHeader, BackupStatus, RestoreCheck, read_backup_header};
use av_application::security::{LockConfig, Throttle, new_lock, validate_idle};
use av_application::{AppCore, RequestContext};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::commands::CommandError;

type CmdResult<T> = Result<T, CommandError>;

fn ctx() -> RequestContext {
    RequestContext::local_owner()
}

fn io_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::new("io_error", format!("{e}"))
}

/// Commands the UI may call while the app is locked.
pub const ALLOWED_WHEN_LOCKED: &[&str] = &[
    "app_session",
    "lock_app",
    "unlock_app",
    "unlock_with_recovery",
];

// ---------------------------------------------------------------------------
// App lock
// ---------------------------------------------------------------------------

/// Lock settings live in `app-lock.json` next to the database, outside it,
/// so restoring a backup never changes or removes the lock.
pub struct LockState {
    file: PathBuf,
    config: Mutex<Option<LockConfig>>,
    locked: AtomicBool,
    throttle: Mutex<Throttle>,
}

impl LockState {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("app-lock.json");
        let config: Option<LockConfig> = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        LockState {
            locked: AtomicBool::new(config.is_some()),
            config: Mutex::new(config),
            file,
            throttle: Mutex::new(Throttle::default()),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    fn save(&self, cfg: Option<&LockConfig>) -> CmdResult<()> {
        match cfg {
            Some(c) => {
                let json = serde_json::to_vec_pretty(c).map_err(io_err)?;
                let tmp = self.file.with_extension("json.tmp");
                std::fs::write(&tmp, json).map_err(io_err)?;
                std::fs::rename(&tmp, &self.file).map_err(io_err)?;
            }
            None => {
                if self.file.exists() {
                    std::fs::remove_file(&self.file).map_err(io_err)?;
                }
            }
        }
        Ok(())
    }

    fn config(&self) -> Option<LockConfig> {
        self.config.lock().expect("lock config").clone()
    }

    /// Checks `pin` against the stored hash, with throttling.
    fn verify(&self, pin: &str) -> CmdResult<()> {
        let cfg = self
            .config()
            .ok_or_else(|| CommandError::new("not_found", "The app lock isn't turned on."))?;
        self.attempt(|| cfg.check_pin(pin), "That PIN isn't right.")
    }

    fn attempt(&self, check: impl FnOnce() -> bool, wrong: &str) -> CmdResult<()> {
        let now = Instant::now();
        if let Some(wait) = self.throttle.lock().expect("throttle").wait(now) {
            return Err(CommandError::new(
                "throttled",
                format!(
                    "Too many wrong attempts. Try again in {} seconds.",
                    wait.as_secs().max(1)
                ),
            ));
        }
        if check() {
            self.throttle.lock().expect("throttle").succeeded();
            Ok(())
        } else {
            self.throttle
                .lock()
                .expect("throttle")
                .failed(Instant::now());
            Err(CommandError::field("pin", wrong))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockStatus {
    enabled: bool,
    locked: bool,
    idle_minutes: u32,
}

fn status(lock: &LockState) -> LockStatus {
    let cfg = lock.config();
    LockStatus {
        enabled: cfg.is_some(),
        locked: lock.is_locked(),
        idle_minutes: cfg.map(|c| c.idle_minutes).unwrap_or(0),
    }
}

/// Set once at start-up when a restore was applied: where the replaced data went.
pub struct StartupNotice(pub Mutex<Option<String>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSession {
    lock: LockStatus,
    /// After a restore: the file the previous data was kept in.
    restored_kept_copy: Option<String>,
}

#[tauri::command]
pub fn app_session(lock: State<'_, LockState>, notice: State<'_, StartupNotice>) -> AppSession {
    AppSession {
        lock: status(&lock),
        restored_kept_copy: notice.0.lock().expect("notice").take(),
    }
}

#[tauri::command]
pub fn lock_app(lock: State<'_, LockState>) -> LockStatus {
    if lock.config().is_some() {
        lock.locked.store(true, Ordering::SeqCst);
    }
    status(&lock)
}

#[tauri::command]
pub async fn unlock_app(lock: State<'_, LockState>, pin: String) -> CmdResult<LockStatus> {
    lock.verify(&pin)?;
    lock.locked.store(false, Ordering::SeqCst);
    Ok(status(&lock))
}

/// The recovery code unlocks and turns the lock off (a new PIN can then be set).
#[tauri::command]
pub async fn unlock_with_recovery(
    lock: State<'_, LockState>,
    code: String,
) -> CmdResult<LockStatus> {
    let cfg = lock
        .config()
        .ok_or_else(|| CommandError::new("not_found", "The app lock isn't turned on."))?;
    lock.attempt(
        || cfg.check_recovery(&code),
        "That recovery code isn't right.",
    )
    .map_err(|e| e.rename_field("pin", "code"))?;
    lock.save(None)?;
    *lock.config.lock().expect("lock config") = None;
    lock.locked.store(false, Ordering::SeqCst);
    Ok(status(&lock))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockEnabled {
    status: LockStatus,
    /// Shown once.
    recovery_code: String,
}

/// Turn the lock on, or change the PIN (the current PIN is required then).
#[tauri::command]
pub async fn set_app_lock(
    lock: State<'_, LockState>,
    current_pin: Option<String>,
    pin: String,
    idle_minutes: u32,
) -> CmdResult<LockEnabled> {
    if lock.config().is_some() {
        lock.verify(current_pin.as_deref().unwrap_or(""))
            .map_err(|e| e.rename_field("pin", "currentPin"))?;
    }
    let created = new_lock(&pin, idle_minutes)?;
    lock.save(Some(&created.config))?;
    *lock.config.lock().expect("lock config") = Some(created.config);
    Ok(LockEnabled {
        status: status(&lock),
        recovery_code: created.recovery_code,
    })
}

#[tauri::command]
pub async fn set_lock_timeout(
    lock: State<'_, LockState>,
    idle_minutes: u32,
) -> CmdResult<LockStatus> {
    validate_idle(idle_minutes)?;
    let mut cfg = lock
        .config()
        .ok_or_else(|| CommandError::new("not_found", "The app lock isn't turned on."))?;
    cfg.idle_minutes = idle_minutes;
    lock.save(Some(&cfg))?;
    *lock.config.lock().expect("lock config") = Some(cfg);
    Ok(status(&lock))
}

#[tauri::command]
pub async fn remove_app_lock(lock: State<'_, LockState>, pin: String) -> CmdResult<LockStatus> {
    lock.verify(&pin)?;
    lock.save(None)?;
    *lock.config.lock().expect("lock config") = None;
    lock.locked.store(false, Ordering::SeqCst);
    Ok(status(&lock))
}

// ---------------------------------------------------------------------------
// Files written this session (only these can be opened or shown again)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SavedFiles(Mutex<HashSet<PathBuf>>);

impl SavedFiles {
    fn remember(&self, p: &Path) {
        self.0.lock().expect("saved files").insert(p.to_path_buf());
    }
    fn known(&self, p: &Path) -> bool {
        self.0.lock().expect("saved files").contains(p)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    path: String,
    file_name: String,
    size_bytes: usize,
}

async fn save_dialog(
    app: &AppHandle,
    file_name: String,
    filter: (&'static str, &'static str),
) -> Option<PathBuf> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app
            .dialog()
            .file()
            .set_file_name(file_name)
            .add_filter(filter.0, &[filter.1]);
        if let Some(w) = app.get_webview_window("main") {
            d = d.set_parent(&w);
        }
        d.blocking_save_file().and_then(|p| p.into_path().ok())
    })
    .await
    .ok()
    .flatten()
}

async fn open_dialog(app: &AppHandle, filter: (&'static str, &'static str)) -> Option<PathBuf> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file().add_filter(filter.0, &[filter.1]);
        if let Some(w) = app.get_webview_window("main") {
            d = d.set_parent(&w);
        }
        d.blocking_pick_file().and_then(|p| p.into_path().ok())
    })
    .await
    .ok()
    .flatten()
}

fn write_file(path: &Path, bytes: &[u8]) -> CmdResult<()> {
    // Write beside the target, then move into place, so a failed write never
    // leaves a half-written file under the chosen name.
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(io_err)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(io_err)?;
    }
    std::fs::rename(&tmp, path).map_err(io_err)?;
    Ok(())
}

fn saved(path: &Path, size: usize) -> SavedFile {
    SavedFile {
        path: path.display().to_string(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size_bytes: size,
    }
}

// ---------------------------------------------------------------------------
// Printed documents
// ---------------------------------------------------------------------------

/// Renders the document and asks where to save it. `None` when cancelled.
#[tauri::command]
pub async fn save_sales_pdf(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    id: String,
) -> CmdResult<Option<SavedFile>> {
    let pdf = core.sales_document_pdf(&ctx(), &id).await?;
    let Some(path) = save_dialog(&app, pdf.file_name.clone(), ("PDF document", "pdf")).await else {
        return Ok(None);
    };
    write_file(&path, &pdf.bytes)?;
    files.remember(&path);
    Ok(Some(saved(&path, pdf.bytes.len())))
}

/// Renders to a temporary file and opens it in the default PDF viewer
/// (for a quick look or printing).
#[tauri::command]
pub async fn open_sales_pdf(app: AppHandle, core: State<'_, AppCore>, id: String) -> CmdResult<()> {
    let pdf = core.sales_document_pdf(&ctx(), &id).await?;
    let dir = std::env::temp_dir().join("AccountsVU");
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    let path = dir.join(&pdf.file_name);
    write_file(&path, &pdf.bytes)?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(io_err)
}

/// Asks where to save, then writes the GSTR-1 portal JSON. `None` when
/// cancelled. The export is audit-logged only once a file is written.
#[tauri::command]
pub async fn save_gstr1_json(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    registration_id: String,
    period: String,
) -> CmdResult<Option<SavedFile>> {
    let view = core.gstr1(&ctx(), &registration_id, &period).await?;
    let (year, month) = view.period.split_once('-').unwrap_or_default();
    let name = format!("GSTR1_{}_{month}{year}.json", view.gstin);
    let Some(path) = save_dialog(&app, name, ("GSTR-1 JSON", "json")).await else {
        return Ok(None);
    };
    let file = core.gstr1_json(&ctx(), &registration_id, &period).await?;
    write_file(&path, file.json.as_bytes())?;
    files.remember(&path);
    Ok(Some(saved(&path, file.json.len())))
}

/// Asks where to save, then writes the GSTR-3B Excel workpaper. `None`
/// when cancelled.
#[tauri::command]
pub async fn save_gstr3b_xlsx(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    registration_id: String,
    period: String,
) -> CmdResult<Option<SavedFile>> {
    let file = core.gstr3b_xlsx(&ctx(), &registration_id, &period).await?;
    let Some(path) = save_dialog(&app, file.file_name.clone(), ("Excel workbook", "xlsx")).await
    else {
        return Ok(None);
    };
    write_file(&path, &file.bytes)?;
    files.remember(&path);
    Ok(Some(saved(&path, file.bytes.len())))
}

/// Asks where to save, then writes the GSTR-2B reconciliation workbook.
/// `None` when cancelled.
#[tauri::command]
pub async fn save_gstr2b_xlsx(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    registration_id: String,
    period: String,
) -> CmdResult<Option<SavedFile>> {
    let file = core.gstr2b_xlsx(&ctx(), &registration_id, &period).await?;
    let Some(path) = save_dialog(&app, file.file_name.clone(), ("Excel workbook", "xlsx")).await
    else {
        return Ok(None);
    };
    write_file(&path, &file.bytes)?;
    files.remember(&path);
    Ok(Some(saved(&path, file.bytes.len())))
}

/// Asks where to save, then writes the filing pack (zip). `None` when cancelled.
#[tauri::command]
pub async fn save_filing_pack(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    registration_id: String,
    period: String,
    return_type: String,
) -> CmdResult<Option<SavedFile>> {
    let file = core
        .filing_pack(&ctx(), &registration_id, &period, &return_type)
        .await?;
    save_zip(&app, &files, file).await
}

/// Asks where to save, then writes the compliance pack (zip) of a filed
/// return. `None` when cancelled.
#[tauri::command]
pub async fn save_compliance_pack(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    registration_id: String,
    period: String,
    return_type: String,
) -> CmdResult<Option<SavedFile>> {
    let file = core
        .compliance_pack(&ctx(), &registration_id, &period, &return_type)
        .await?;
    save_zip(&app, &files, file).await
}

async fn save_zip(
    app: &AppHandle,
    files: &SavedFiles,
    file: av_application::gstr3b::WorkpaperFile,
) -> CmdResult<Option<SavedFile>> {
    let Some(path) = save_dialog(app, file.file_name.clone(), ("Zip archive", "zip")).await else {
        return Ok(None);
    };
    write_file(&path, &file.bytes)?;
    files.remember(&path);
    Ok(Some(saved(&path, file.bytes.len())))
}

/// Asks where to save, then writes the import template (CSV with a BOM so
/// Excel reads it as UTF-8). `None` when cancelled.
#[tauri::command]
pub async fn save_import_template(
    app: AppHandle,
    files: State<'_, SavedFiles>,
    kind: av_application::ImportKind,
) -> CmdResult<Option<SavedFile>> {
    let t = av_application::import_template(kind);
    let Some(path) = save_dialog(&app, t.file_name.clone(), ("CSV file", "csv")).await else {
        return Ok(None);
    };
    let bytes = [&[0xEF_u8, 0xBB, 0xBF][..], t.csv.as_bytes()].concat();
    write_file(&path, &bytes)?;
    files.remember(&path);
    Ok(Some(saved(&path, bytes.len())))
}

/// Opens a file this session saved (never an arbitrary path).
#[tauri::command]
pub async fn open_saved_file(
    app: AppHandle,
    files: State<'_, SavedFiles>,
    path: String,
) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    if !files.known(&p) {
        return Err(CommandError::new(
            "forbidden",
            "Only files saved by AccountsVU can be opened here.",
        ));
    }
    app.opener().reveal_item_in_dir(&p).map_err(io_err)
}

// ---------------------------------------------------------------------------
// Backup and restore
// ---------------------------------------------------------------------------

pub struct DataPaths {
    pub dir: PathBuf,
    pub db: PathBuf,
}

impl DataPaths {
    pub fn staging(&self) -> PathBuf {
        self.dir.join("restore-staging.sqlite3")
    }
}

/// The backup file chosen for restore, and whether it has been checked.
#[derive(Default)]
pub struct RestoreState(Mutex<Option<(PathBuf, bool)>>);

#[tauri::command]
pub async fn backup_status(core: State<'_, AppCore>) -> CmdResult<BackupStatus> {
    Ok(core.backup_status(&ctx()).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDone {
    file: SavedFile,
    status: BackupStatus,
}

/// Asks where to save, then writes an encrypted backup. `None` when cancelled.
#[tauri::command]
pub async fn create_backup(
    app: AppHandle,
    core: State<'_, AppCore>,
    files: State<'_, SavedFiles>,
    passphrase: String,
) -> CmdResult<Option<BackupDone>> {
    // Validate (and check permission) before showing the dialog.
    let backup = core.create_backup(&ctx(), &passphrase).await?;
    let Some(path) = save_dialog(
        &app,
        backup.file_name.clone(),
        ("AccountsVU backup", "avbackup"),
    )
    .await
    else {
        return Ok(None);
    };
    write_file(&path, &backup.bytes)?;
    // Read it back and make sure it opens before calling it done.
    let written = std::fs::read(&path).map_err(io_err)?;
    if written != backup.bytes {
        return Err(io_err(
            "the backup file didn't save correctly; try another folder",
        ));
    }
    let name = saved(&path, written.len());
    let status = core
        .record_backup(&ctx(), &name.file_name, &written)
        .await?;
    files.remember(&path);
    Ok(Some(BackupDone { file: name, status }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChosenBackup {
    file_name: String,
    size_bytes: u64,
    header: BackupHeader,
}

/// Step 1 of restore: pick a file; its readable header is shown.
#[tauri::command]
pub async fn choose_backup_file(
    app: AppHandle,
    core: State<'_, AppCore>,
    restore: State<'_, RestoreState>,
) -> CmdResult<Option<ChosenBackup>> {
    core.require_restore_permission(&ctx()).await?;
    let Some(path) = open_dialog(&app, ("AccountsVU backup", "avbackup")).await else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(io_err)?;
    let (header, _) = read_backup_header(&bytes)?;
    *restore.0.lock().expect("restore") = Some((path.clone(), false));
    Ok(Some(ChosenBackup {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size_bytes: bytes.len() as u64,
        header,
    }))
}

/// Step 2: decrypt with the passphrase and check the database inside.
#[tauri::command]
pub async fn verify_backup(
    core: State<'_, AppCore>,
    restore: State<'_, RestoreState>,
    paths: State<'_, DataPaths>,
    passphrase: String,
) -> CmdResult<RestoreCheck> {
    let path = restore
        .0
        .lock()
        .expect("restore")
        .as_ref()
        .map(|(p, _)| p.clone())
        .ok_or_else(|| CommandError::new("not_found", "Choose a backup file first."))?;
    let bytes = std::fs::read(&path).map_err(io_err)?;
    let check = core
        .prepare_restore(&ctx(), &bytes, &passphrase, &paths.staging())
        .await?;
    *restore.0.lock().expect("restore") = Some((path, true));
    Ok(check)
}

/// Step 3: queue the checked database and restart; it replaces the current
/// data before the database is opened again.
#[tauri::command]
pub async fn apply_restore(
    app: AppHandle,
    core: State<'_, AppCore>,
    restore: State<'_, RestoreState>,
    paths: State<'_, DataPaths>,
) -> CmdResult<()> {
    core.require_restore_permission(&ctx()).await?;
    let checked = matches!(*restore.0.lock().expect("restore"), Some((_, true)));
    let staging = paths.staging();
    if !checked || !staging.exists() {
        return Err(CommandError::new(
            "conflict",
            "Check the backup with its passphrase first.",
        ));
    }
    let pending = av_persistence::sqlite::pending_restore_path(&paths.db);
    std::fs::rename(&staging, &pending).map_err(io_err)?;
    core.close().await;
    app.restart();
}

#[tauri::command]
pub async fn cancel_restore(
    restore: State<'_, RestoreState>,
    paths: State<'_, DataPaths>,
) -> CmdResult<()> {
    *restore.0.lock().expect("restore") = None;
    let _ = std::fs::remove_file(paths.staging());
    Ok(())
}
