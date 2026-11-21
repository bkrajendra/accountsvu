use super::*;

const TS: &str = "2026-11-17T00:00:00Z";

async fn store_with_orgs(ids: &[&str]) -> SqliteStore {
    let store = SqliteStore::open_in_memory().await.unwrap();
    for id in ids {
        sqlx::query("INSERT INTO organisation (id, legal_name, state_code, created_at, updated_at) VALUES (?1, 'Test', 27, ?2, ?2)")
            .bind(id)
            .bind(TS)
            .execute(store.pool())
            .await
            .unwrap();
    }
    store
}

fn series(id: &str, org: &str, doc: &str, prefix: &str) -> SeriesRecord {
    SeriesRecord {
        id: id.into(),
        organisation_id: org.into(),
        gst_registration_id: None,
        document_type: doc.into(),
        financial_year: "2026-27".into(),
        prefix: prefix.into(),
        suffix: String::new(),
        padding: 4,
        next_number: 1,
        is_default: false,
        row_version: 1,
        created_at: TS.into(),
        updated_at: TS.into(),
    }
}

#[tokio::test]
async fn migrates_and_reports_version() {
    let store = store_with_orgs(&[]).await;
    assert_eq!(store.schema_version().await.unwrap(), 10);
    let mut u = store.begin().await.unwrap();
    assert_eq!(u.organisation_count().await.unwrap(), 0);
    assert_eq!(
        u.role_balance_paise("o", "receivables", None, None)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn series_numbers_are_unique_and_sequential() {
    let store = store_with_orgs(&["o"]).await;
    let mut u = store.begin().await.unwrap();
    u.insert_series(&series("s1", "o", "tax_invoice", "INV/"))
        .await
        .unwrap();
    // Same prefix in the same FY collides even for a different document type.
    assert!(
        u.insert_series(&series("s2", "o", "credit_note", "INV/"))
            .await
            .is_err()
    );
    assert!(
        u.series_scope_taken("o", None, "2026-27", "INV/", "", None)
            .await
            .unwrap()
    );
    assert!(
        !u.series_scope_taken("o", None, "2026-27", "INV/", "", Some("s1"))
            .await
            .unwrap()
    );

    assert_eq!(u.reserve_next_number("o", "s1", TS).await.unwrap(), 1);
    assert_eq!(u.reserve_next_number("o", "s1", TS).await.unwrap(), 2);
    let s = u.get_series("o", "s1").await.unwrap().unwrap();
    assert_eq!(s.next_number, 3);

    // Stale version is rejected.
    let mut edited = s.clone();
    edited.prefix = "BILL/".into();
    assert!(!u.update_series(&edited, 1).await.unwrap());
    assert!(u.update_series(&edited, s.row_version).await.unwrap());
    u.commit().await.unwrap();
}

#[tokio::test]
async fn dropping_a_unit_of_work_rolls_back() {
    let store = store_with_orgs(&["o"]).await;
    {
        let mut u = store.begin().await.unwrap();
        u.insert_series(&series("s1", "o", "tax_invoice", "INV/"))
            .await
            .unwrap();
    }
    let mut u = store.begin().await.unwrap();
    assert!(u.get_series("o", "s1").await.unwrap().is_none());
}

#[tokio::test]
async fn organisations_cannot_see_or_change_each_others_rows() {
    let store = store_with_orgs(&["a", "b"]).await;
    let mut u = store.begin().await.unwrap();
    u.insert_series(&series("sa", "a", "tax_invoice", "INV/"))
        .await
        .unwrap();

    assert!(u.get_series("b", "sa").await.unwrap().is_none());
    assert!(u.list_series("b").await.unwrap().is_empty());
    assert!(
        !u.series_scope_taken("b", None, "2026-27", "INV/", "", None)
            .await
            .unwrap()
    );
    // The same prefix is fine in another organisation.
    u.insert_series(&series("sb", "b", "tax_invoice", "INV/"))
        .await
        .unwrap();
    // Updating with the wrong organisation touches nothing.
    let mut forged = u.get_series("a", "sa").await.unwrap().unwrap();
    forged.organisation_id = "b".into();
    forged.prefix = "HACK/".into();
    assert!(!u.update_series(&forged, 1).await.unwrap());
    assert!(u.reserve_next_number("b", "sa", TS).await.is_err());
    assert_eq!(
        u.get_series("a", "sa").await.unwrap().unwrap().prefix,
        "INV/"
    );
}

#[tokio::test]
async fn journal_lines_are_immutable_and_one_sided() {
    let store = store_with_orgs(&["o"]).await;
    let p = store.pool();
    sqlx::query("INSERT INTO account (id, organisation_id, code, name, kind, created_at) VALUES ('a', 'o', '1000', 'Cash', 'asset', ?1)")
        .bind(TS).execute(p).await.unwrap();
    sqlx::query("INSERT INTO journal_entry (id, organisation_id, entry_date, source_type, posted_at) VALUES ('e', 'o', '2026-11-17', 'manual', ?1)")
        .bind(TS).execute(p).await.unwrap();
    sqlx::query("INSERT INTO journal_line (id, journal_entry_id, line_no, account_id, debit_paise) VALUES ('l', 'e', 1, 'a', 100)")
        .execute(p).await.unwrap();

    let both = sqlx::query("INSERT INTO journal_line (id, journal_entry_id, line_no, account_id, debit_paise, credit_paise) VALUES ('l2', 'e', 2, 'a', 1, 1)")
        .execute(p).await;
    assert!(both.is_err(), "a line cannot carry both debit and credit");

    let upd = sqlx::query("UPDATE journal_line SET debit_paise = 5 WHERE id = 'l'")
        .execute(p)
        .await;
    assert!(upd.is_err(), "posted lines must be immutable");

    let mut u = store.begin().await.unwrap();
    assert_eq!(u.journal_entry_count("o").await.unwrap(), 1);
    assert_eq!(u.journal_entry_count("other").await.unwrap(), 0);
    assert_eq!(
        u.role_balance_paise("o", "cash", None, None).await.unwrap(),
        0,
        "account has no role"
    );
}

#[tokio::test]
async fn issued_sales_documents_are_frozen_by_the_database() {
    let store = store_with_orgs(&["o"]).await;
    let p = store.pool();
    for sql in [
        "INSERT INTO party (id, organisation_id, kind, name, gst_type, state_code, created_at, updated_at) VALUES ('c', 'o', 'customer', 'C', 'consumer', 27, 't', 't')",
        "INSERT INTO journal_entry (id, organisation_id, entry_date, source_type, posted_at) VALUES ('j', 'o', '2026-11-17', 'sales_invoice', 't')",
        "INSERT INTO sales_document (id, organisation_id, doc_type, status, doc_date, party_id, party_name, party_gst_type, place_of_supply, supplier_state, supply_kind, total_paise, created_at, updated_at) VALUES ('d', 'o', 'tax_invoice', 'draft', '2026-11-17', 'c', 'C', 'consumer', 27, 27, 'intra_state', 100, 't', 't')",
        "INSERT INTO sales_line (id, document_id, line_no, description, quantity, unit_price_paise, tax_category, amount_paise, taxable_paise, total_paise) VALUES ('l', 'd', 1, 'x', '1', 100, 'exempt', 100, 100, 100)",
    ] {
        sqlx::query(sql).execute(p).await.unwrap();
    }
    // Issuing needs a number and a posting.
    assert!(
        sqlx::query("UPDATE sales_document SET status = 'issued' WHERE id = 'd'")
            .execute(p)
            .await
            .is_err()
    );
    sqlx::query("UPDATE sales_document SET status = 'issued', number = 'INV/1', journal_entry_id = 'j' WHERE id = 'd'")
        .execute(p)
        .await
        .unwrap();

    for sql in [
        "UPDATE sales_document SET total_paise = 1 WHERE id = 'd'",
        "UPDATE sales_document SET number = 'INV/2' WHERE id = 'd'",
        "UPDATE sales_document SET status = 'draft' WHERE id = 'd'",
        "UPDATE sales_line SET amount_paise = 1 WHERE id = 'l'",
        "DELETE FROM sales_line WHERE id = 'l'",
        "INSERT INTO sales_line (id, document_id, line_no, description, quantity, unit_price_paise, tax_category, amount_paise, taxable_paise, total_paise) VALUES ('l2', 'd', 2, 'x', '1', 1, 'exempt', 1, 1, 1)",
        "DELETE FROM sales_document WHERE id = 'd'",
        // Cancelling needs a reason and a reversal entry.
        "UPDATE sales_document SET status = 'cancelled' WHERE id = 'd'",
    ] {
        assert!(
            sqlx::query(sql).execute(p).await.is_err(),
            "should be refused: {sql}"
        );
    }
}

#[tokio::test]
async fn recorded_purchases_are_frozen_except_itc_review() {
    let store = store_with_orgs(&["o"]).await;
    let p = store.pool();
    for sql in [
        "INSERT INTO party (id, organisation_id, kind, name, gst_type, state_code, created_at, updated_at) VALUES ('s', 'o', 'supplier', 'S', 'regular', 27, 't', 't')",
        "INSERT INTO account (id, organisation_id, code, name, kind, created_at) VALUES ('a', 'o', '5000', 'Purchases', 'expense', 't')",
        "INSERT INTO journal_entry (id, organisation_id, entry_date, source_type, posted_at) VALUES ('j', 'o', '2026-11-17', 'purchase', 't')",
        "INSERT INTO purchase_document (id, organisation_id, doc_type, status, doc_date, party_id, place_of_supply, supplier_state, supply_kind, payment_mode, total_paise, created_at, updated_at) VALUES ('d', 'o', 'bill', 'draft', '2026-11-17', 's', 27, 27, 'intra_state', 'credit', 118, 't', 't')",
        "INSERT INTO purchase_line (id, document_id, line_no, account_id, description, quantity, unit_price_paise, tax_category, amount_paise, taxable_paise, cgst_paise, sgst_paise, total_paise, itc_status) VALUES ('l', 'd', 1, 'a', 'x', '1', 100, 'taxable', 100, 100, 9, 9, 118, 'pending')",
        "UPDATE purchase_document SET status = 'recorded', journal_entry_id = 'j' WHERE id = 'd'",
        // ITC review is allowed on a recorded line.
        "UPDATE purchase_line SET itc_status = 'eligible', itc_eligible_paise = 18 WHERE id = 'l'",
    ] {
        sqlx::query(sql).execute(p).await.unwrap();
    }
    for sql in [
        "UPDATE purchase_line SET taxable_paise = 1 WHERE id = 'l'",
        "UPDATE purchase_line SET account_id = 'a', description = 'changed' WHERE id = 'l'",
        "UPDATE purchase_document SET total_paise = 1 WHERE id = 'd'",
        "DELETE FROM purchase_document WHERE id = 'd'",
        "INSERT INTO attachment_blob (sha256, content, size_bytes) VALUES ('h', x'00', 1)",
        "UPDATE attachment_blob SET content = x'01' WHERE sha256 = 'h'",
    ] {
        let r = sqlx::query(sql).execute(p).await;
        if sql.starts_with("INSERT") {
            r.unwrap();
        } else {
            assert!(r.is_err(), "should be refused: {sql}");
        }
    }
}

#[tokio::test]
async fn snapshot_writes_a_checkable_copy() {
    let dir = std::env::temp_dir().join(format!("av-snap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SqliteStore::open(&dir.join("live.sqlite3")).await.unwrap();
    let dest = dir.join("copy.sqlite3");
    let _ = std::fs::remove_file(&dest);
    store.snapshot(&dest).await.unwrap();
    assert!(dest.exists(), "snapshot file missing at {}", dest.display());
    let check = crate::sqlite::inspect_database(&dest).await.unwrap();
    assert!(check.is_sound());
    assert_eq!(check.schema_version, 10);
    assert!(store.snapshot(&dest).await.is_err(), "never overwrites");
    let memory = SqliteStore::open_in_memory().await.unwrap();
    assert!(memory.snapshot(&dir.join("mem.sqlite3")).await.is_err());
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn closing_releases_the_file() {
    let dir = std::env::temp_dir().join(format!("av-close-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("db.sqlite3");
    let store = SqliteStore::open(&path).await.unwrap();
    store.schema_version().await.unwrap();
    store.close().await;
    let moved = dir.join("moved.sqlite3");
    let mut ok = false;
    for i in 0..40 {
        if std::fs::rename(&path, &moved).is_ok() {
            ok = i < 40;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(ok);
    let _ = std::fs::remove_dir_all(&dir);
}
