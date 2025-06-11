#![deny(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::module_name_repetitions)]

//! A Tonbo-backed database implementation for Fedimint.
//!
//! This crate provides a database backend using Tonbo, an embedded persistent
//! KV database written in Rust. It implements the Fedimint database traits to
//! provide persistent storage.
//!
//! # Limitations
//!
//! - Does not support transaction savepoints
//! - Module isolation (prefix databases) is not implemented

use std::fmt;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use fedimint_core::async_trait_maybe_send;
use fedimint_core::db::{
    IDatabaseTransactionOps, IDatabaseTransactionOpsCore, IRawDatabase, IRawDatabaseTransaction,
    PrefixStream,
};
use futures::StreamExt;
use macro_rules_attribute::apply;
use tonbo::executor::tokio::TokioExecutor;
use tonbo::option::Path as TonboPath;
use tonbo::transaction::Transaction;
use tonbo::{DbOption, Projection, Record, DB};

/// Key-value pair schema for Fedimint storage
#[derive(Record, Debug, Clone)]
pub struct KvPair {
    #[record(primary_key)]
    key: Vec<u8>,
    value: Vec<u8>,
}

/// Tonbo database implementation for Fedimint
pub struct TonboDatabase {
    db: Arc<DB<KvPair, TokioExecutor>>,
}

impl TonboDatabase {
    pub async fn new(path: &Path) -> Result<Self> {
        let db_path = TonboPath::from_filesystem_path(path)?;

        let options = DbOption::new(db_path, &KvPairSchema)
            .level_sst_magnification(8)
            .max_sst_file_size(128 * 1024 * 1024); // 128MB

        let db = DB::new(options, TokioExecutor::current(), KvPairSchema).await?;

        Ok(Self { db: Arc::new(db) })
    }
}

impl fmt::Debug for TonboDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TonboDatabase").finish()
    }
}

/// Tonbo transaction wrapper for Fedimint
pub struct TonboTransaction<'a> {
    txn: Transaction<'a, KvPair>,
}

impl<'a> fmt::Debug for TonboTransaction<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TonboTransaction").finish()
    }
}

#[apply(async_trait_maybe_send!)]
impl IRawDatabase for TonboDatabase {
    type Transaction<'a> = TonboTransaction<'a>;

    async fn begin_transaction<'a>(&'a self) -> TonboTransaction<'a> {
        let txn = self.db.transaction().await;
        TonboTransaction { txn }
    }

    fn checkpoint(&self, _backup_path: &Path) -> Result<()> {
        // Tonbo handles checkpointing internally through compaction
        Ok(())
    }
}

#[apply(async_trait_maybe_send!)]
impl<'a> IDatabaseTransactionOpsCore for TonboTransaction<'a> {
    async fn raw_insert_bytes(&mut self, key: &[u8], value: &[u8]) -> Result<Option<Vec<u8>>> {
        let key_bytes = Bytes::copy_from_slice(key);

        // First, try to get the old value
        let old_value = match self.txn.get(&key_bytes, Projection::All).await? {
            Some(entry) => Some(entry.get().value.to_vec()),
            None => None,
        };

        // Insert the new record
        let record = KvPair {
            key: key_bytes,
            value: Bytes::copy_from_slice(value),
        };

        self.txn.insert(record);

        Ok(old_value)
    }

    async fn raw_get_bytes(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let key_bytes = Bytes::copy_from_slice(key);

        let result = match self.txn.get(&key_bytes, Projection::All).await? {
            Some(entry) => Some(entry.get().value.to_vec()),
            None => None,
        };

        Ok(result)
    }

    async fn raw_remove_entry(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // Get the old value first
        let old_value = self.raw_get_bytes(key).await?;

        if old_value.is_some() {
            let key_bytes = Bytes::copy_from_slice(key);
            self.txn.remove(key_bytes);
        }

        Ok(old_value)
    }

    async fn raw_find_by_prefix(&mut self, key_prefix: &[u8]) -> Result<PrefixStream<'_>> {
        let prefix = key_prefix.to_vec();
        let lower_bytes = Bytes::copy_from_slice(&prefix);

        // Create upper bound by incrementing the last byte
        let mut upper_vec = prefix.clone();
        let mut found = false;
        for byte in upper_vec.iter_mut().rev() {
            if *byte < 255 {
                *byte += 1;
                found = true;
                break;
            }
            *byte = 0;
        }

        let upper_bytes = if found {
            Some(Bytes::copy_from_slice(&upper_vec))
        } else {
            None
        };

        let range = if let Some(ref upper) = upper_bytes {
            (
                std::ops::Bound::Included(&lower_bytes),
                std::ops::Bound::Excluded(upper),
            )
        } else {
            // If all bytes are 255, use unbounded upper
            (
                std::ops::Bound::Included(&lower_bytes),
                std::ops::Bound::Unbounded,
            )
        };

        // Collect all results first
        let mut results = Vec::new();
        if let Ok(mut scan_stream) = self.txn.scan(range).take().await {
            while let Some(result) = scan_stream.next().await {
                if let Ok(entry) = result {
                    if let Some(record_ref) = entry.value() {
                        let key = &record_ref.key;
                        // Double-check prefix match
                        if key.starts_with(&prefix) {
                            results.push((key.to_vec(), record_ref.value.to_vec()));
                        }
                    }
                }
            }
        }

        Ok(Box::pin(futures::stream::iter(results)))
    }

    async fn raw_find_by_prefix_sorted_descending(
        &mut self,
        key_prefix: &[u8],
    ) -> Result<PrefixStream<'_>> {
        // Collect all matching entries and sort them
        let mut entries = Vec::new();
        let mut stream = self.raw_find_by_prefix(key_prefix).await?;

        while let Some((key, value)) = stream.next().await {
            entries.push((key, value));
        }

        // Sort in descending order
        entries.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(Box::pin(futures::stream::iter(entries)))
    }

    async fn raw_find_by_range(&mut self, range: Range<&[u8]>) -> Result<PrefixStream<'_>> {
        let lower_bytes = Bytes::copy_from_slice(range.start);
        let upper_bytes = Bytes::copy_from_slice(range.end);

        let scan_range = (
            std::ops::Bound::Included(&lower_bytes),
            std::ops::Bound::Excluded(&upper_bytes),
        );

        // Collect all results first
        let mut results = Vec::new();
        if let Ok(mut scan_stream) = self.txn.scan(scan_range).take().await {
            while let Some(result) = scan_stream.next().await {
                if let Ok(entry) = result {
                    if let Some(record_ref) = entry.value() {
                        results.push((record_ref.key.to_vec(), record_ref.value.to_vec()));
                    }
                }
            }
        }

        Ok(Box::pin(futures::stream::iter(results)))
    }

    async fn raw_remove_by_prefix(&mut self, key_prefix: &[u8]) -> Result<()> {
        let keys: Vec<Vec<u8>> = self
            .raw_find_by_prefix(key_prefix)
            .await?
            .map(|(k, _)| k)
            .collect()
            .await;

        for key in keys {
            self.raw_remove_entry(&key).await?;
        }

        Ok(())
    }
}

#[apply(async_trait_maybe_send!)]
impl<'a> IDatabaseTransactionOps for TonboTransaction<'a> {
    async fn rollback_tx_to_savepoint(&mut self) -> Result<()> {
        // Tonbo doesn't support savepoints within transactions
        // This is a limitation that needs to be handled at a higher level
        Err(anyhow!("Tonbo does not support transaction savepoints"))
    }

    async fn set_tx_savepoint(&mut self) -> Result<()> {
        // Tonbo doesn't support savepoints within transactions
        // This is a limitation that needs to be handled at a higher level
        Err(anyhow!("Tonbo does not support transaction savepoints"))
    }
}

#[apply(async_trait_maybe_send!)]
impl<'a> IRawDatabaseTransaction for TonboTransaction<'a> {
    async fn commit_tx(self) -> Result<()> {
        self.txn.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
