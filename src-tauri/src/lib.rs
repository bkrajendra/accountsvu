//! Tauri desktop host. Kept deliberately thin: every command validates its
//! input shape and delegates to `av_application::AppCore` (spec D8).

mod commands;
mod host;
mod hsn;
mod updater;

use std::sync::Mutex;

use av_application::{AppCore, RequestContext};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
        commands::app_health,
        commands::dashboard_summary,
        commands::business_profile,
        commands::setup_business,
        commands::update_organisation,
        commands::add_gst_registration,
        commands::update_gst_registration,
        commands::list_accounts,
        commands::list_series,
        commands::create_series,
        commands::update_series,
        commands::list_audit_events,
        commands::list_parties,
        commands::create_party,
        commands::update_party,
        commands::item_options,
        commands::hsn_lookup,
        commands::rule_pack_info,
        commands::list_items,
        commands::create_item,
        commands::update_item,
        commands::import_template,
        commands::parse_import,
        commands::preview_import,
        commands::commit_import,
        commands::list_sales,
        commands::get_sales_document,
        commands::calculate_sales,
        commands::save_sales_draft,
        commands::issue_sales_document,
        commands::cancel_sales_document,
        commands::delete_sales_draft,
        commands::purchase_accounts,
        commands::list_purchases,
        commands::get_purchase,
        commands::calculate_purchase,
        commands::save_purchase_draft,
        commands::record_purchase,
        commands::review_itc,
        commands::cancel_purchase,
        commands::delete_purchase_draft,
        commands::add_attachment,
        commands::attachment_content,
        commands::remove_attachment,
        commands::list_money_accounts,
        commands::create_money_account,
        commands::update_money_account,
        commands::list_opening_balances,
        commands::set_opening_balance,
        commands::voucher_accounts,
        commands::open_documents,
        commands::list_vouchers,
        commands::get_voucher,
        commands::document_settlements,
        commands::create_voucher,
        commands::adjust_advance,
        commands::party_advances,
        commands::cancel_voucher,
        commands::money_book,
        commands::read_statement_file,
        commands::preview_statement,
        commands::import_statement,
        commands::reconciliation,
        commands::match_statement_line,
        commands::unmatch_statement_line,
        commands::ignore_statement_line,
        commands::undo_statement_import,
        commands::trial_balance,
        commands::profit_and_loss,
        commands::balance_sheet,
        commands::general_ledger,
        commands::party_ledger,
        commands::journal_register,
        commands::ageing,
        commands::return_centre,
        commands::gstr1,
        commands::gstr3b,
        commands::gstr2b,
        commands::import_gstr2b,
        commands::decide_gstr2b,
        commands::return_filing,
        commands::record_filing,
        commands::remove_filing,
        commands::books_control,
        commands::set_books_lock,
        commands::close_financial_year,
        commands::reopen_financial_year,
        commands::set_logo,
        commands::get_logo,
        commands::remove_logo,
        hsn::hsn_directory_status,
        hsn::search_hsn_directory,
        hsn::sync_hsn_directory,
        hsn::import_hsn_master_file,
        host::app_session,
        host::lock_app,
        host::unlock_app,
        host::unlock_with_recovery,
        host::set_app_lock,
        host::set_lock_timeout,
        host::remove_app_lock,
        host::save_sales_pdf,
        host::save_gstr1_json,
        host::save_gstr3b_xlsx,
        host::save_import_template,
        host::save_gstr2b_xlsx,
        host::save_filing_pack,
        host::save_compliance_pack,
        host::open_sales_pdf,
        host::save_voucher_pdf,
        host::open_voucher_pdf,
        host::open_saved_file,
        host::backup_status,
        host::create_backup,
        host::choose_backup_file,
        host::verify_backup,
        host::apply_restore,
        host::cancel_restore,
        updater::check_update,
        updater::install_update,
        updater::open_releases_page,
        updater::open_support_page,
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("accountsvu.sqlite3");
            let paths = host::DataPaths {
                dir: data_dir.clone(),
                db: db_path.clone(),
            };
            // A checked restore waiting from the last run replaces the data
            // before the database is opened; a half-finished check is dropped.
            let _ = std::fs::remove_file(paths.staging());
            let stamp = chrono_stamp();
            let kept = av_persistence::sqlite::apply_pending_restore(&db_path, &stamp)?;
            let mut core = tauri::async_runtime::block_on(AppCore::open(&db_path))?;
            // The HSN/SAC directory is a lookup aid in its own file; the app
            // works without it.
            let hsn_path = data_dir.join(av_persistence::hsn_directory::FILE_NAME);
            match tauri::async_runtime::block_on(av_persistence::HsnDirectory::open(&hsn_path)) {
                Ok(d) => core = core.with_hsn_directory(d),
                Err(e) => tracing::warn!(error = %e, "could not open the HSN/SAC directory"),
            }
            let kept_name = kept
                .as_ref()
                .and_then(|k| k.file_name())
                .map(|n| n.to_string_lossy().to_string());
            if kept.is_some() {
                let ctx = RequestContext::local_owner();
                if let Err(e) =
                    tauri::async_runtime::block_on(core.record_restore(&ctx, kept_name.as_deref()))
                {
                    tracing::warn!(error = %e, "could not record the restore");
                }
            }
            app.manage(core);
            app.manage(paths);
            app.manage(host::LockState::load(&data_dir));
            app.manage(host::StartupNotice(Mutex::new(kept_name)));
            app.manage(host::SavedFiles::default());
            app.manage(host::RestoreState::default());
            app.manage(updater::PendingUpdate::default());
            Ok(())
        })
        // Every command passes the app lock first: while locked, only the
        // unlock commands get through (spec D14).
        .invoke_handler(move |invoke| {
            let cmd = invoke.message.command();
            let locked = invoke
                .message
                .webview_ref()
                .try_state::<host::LockState>()
                .is_some_and(|l| l.is_locked());
            if locked && !host::ALLOWED_WHEN_LOCKED.contains(&cmd) {
                invoke.resolver.reject(commands::CommandError::locked());
                return true;
            }
            handler(invoke)
        })
        .run(tauri::generate_context!())
        .expect("error while running AccountsVU");
}

/// Local time stamp for file names, e.g. "20261119-071205".
fn chrono_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}
