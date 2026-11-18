//! Tauri desktop host. Kept deliberately thin: every command validates its
//! input shape and delegates to `av_application::AppCore` (spec D8).

mod commands;

use av_application::AppCore;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db_path = data_dir.join("accountsvu.sqlite3");
            let core = tauri::async_runtime::block_on(AppCore::open(&db_path))?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running AccountsVU");
}
