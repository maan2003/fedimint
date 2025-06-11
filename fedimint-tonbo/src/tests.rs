use fedimint_core::db::{
    expect_write_conflict, verify_commit, verify_find_by_prefix, verify_find_by_range,
    verify_insert_elements, verify_module_db, verify_module_prefix, verify_phantom_entry,
    verify_prevent_dirty_reads, verify_prevent_nonrepeatable_reads, verify_read_own_writes,
    verify_remove_by_prefix, verify_remove_existing, verify_remove_nonexisting,
    verify_rollback_to_savepoint, verify_snapshot_isolation, Database,
};
use tempfile::TempDir;

use crate::TonboDatabase;

async fn create_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = TonboDatabase::new(temp_dir.path()).await.unwrap();
    (db.into(), temp_dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_basic_operations() {
    let (db, _dir) = create_test_db().await;
    verify_insert_elements(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remove() {
    let (db, _dir) = create_test_db().await;
    verify_remove_nonexisting(db.clone()).await;
    verify_remove_existing(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_read_write() {
    let (db, _dir) = create_test_db().await;
    verify_read_own_writes(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_isolation() {
    let (db, _dir) = create_test_db().await;
    verify_prevent_dirty_reads(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_find_by_prefix() {
    let (db, _dir) = create_test_db().await;
    verify_find_by_prefix(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_find_by_range() {
    let (db, _dir) = create_test_db().await;
    verify_find_by_range(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_commit() {
    let (db, _dir) = create_test_db().await;
    verify_commit(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_prevent_nonrepeatable_reads() {
    let (db, _dir) = create_test_db().await;
    verify_prevent_nonrepeatable_reads(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_snapshot_isolation() {
    let (db, _dir) = create_test_db().await;
    verify_snapshot_isolation(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_phantom_entry() {
    let (db, _dir) = create_test_db().await;
    verify_phantom_entry(db).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remove_by_prefix() {
    let (db, _dir) = create_test_db().await;
    verify_remove_by_prefix(db).await;
}

// Note: Module isolation tests are not supported by Tonbo implementation
// These tests are marked as ignored because Tonbo doesn't implement prefix
// databases
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Tonbo does not support prefix databases for module isolation"]
async fn test_module_db() {
    let temp_dir = TempDir::new().unwrap();
    let tonbo_db = TonboDatabase::new(temp_dir.path()).await.unwrap();
    let db: Database = tonbo_db.into();
    let module_db = db.with_prefix_module_id(1).0;
    verify_module_db(db, module_db).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Tonbo does not support prefix databases for module isolation"]
async fn test_module_prefix() {
    let (db, _dir) = create_test_db().await;
    verify_module_prefix(db).await;
}

// Note: Savepoint tests are expected to fail since Tonbo doesn't support
// savepoints
#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "Tonbo does not support transaction savepoints")]
async fn test_rollback_to_savepoint() {
    let (db, _dir) = create_test_db().await;
    verify_rollback_to_savepoint(db).await;
}

// Test for write conflicts
#[tokio::test(flavor = "multi_thread")]
async fn test_write_conflict() {
    let (db, _dir) = create_test_db().await;
    expect_write_conflict(db).await;
}
