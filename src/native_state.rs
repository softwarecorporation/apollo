//! Experimental local state publication. The bundled engine is a synthetic fixture.
//! This is not an authentication boundary or a Lyceus learning interface.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

const MODEL: &str = "synthetic-fixture-model-v1";
const TOKENIZER: &str = "none";
const SCHEMA: &str = "synthetic-two-values-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    pub model: String,
    pub tokenizer: String,
    pub schema: String,
    pub profile: String,
    pub generation: i64,
    pub state_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: Version,
    pub values: [i32; 2],
    pub source: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn open(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch(
        "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS snapshots (
          profile TEXT NOT NULL, generation INTEGER NOT NULL,
          model TEXT NOT NULL, tokenizer TEXT NOT NULL, schema TEXT NOT NULL,
          state BLOB NOT NULL, digest TEXT NOT NULL, source TEXT NOT NULL,
          PRIMARY KEY(profile, generation));",
    )?;
    Ok(db)
}

fn load(db: &Connection, profile: &str) -> Result<Option<Snapshot>> {
    let row = db.query_row(
        "SELECT generation, model, tokenizer, schema, state, digest, source FROM snapshots WHERE profile=?1 ORDER BY generation DESC LIMIT 1",
        [profile],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Vec<u8>>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?)),
    ).optional()?;
    let Some((generation, model, tokenizer, schema, state, digest, source)) = row else {
        return Ok(None);
    };
    if model != MODEL || tokenizer != TOKENIZER || schema != SCHEMA || hash(&state) != digest {
        bail!("state identity or integrity verification failed");
    }
    let values: [i32; 2] = serde_json::from_slice(&state).context("invalid fixture state")?;
    if values.iter().any(|v| !(-1000..=1000).contains(v)) {
        bail!("fixture state out of bounds");
    }
    Ok(Some(Snapshot {
        version: Version {
            model,
            tokenizer,
            schema,
            profile: profile.into(),
            generation,
            state_sha256: digest,
        },
        values,
        source,
    }))
}

/// Read the latest verified synthetic profile. No profile name is an authorization credential.
pub fn read(path: &Path, profile: &str) -> Result<Option<Snapshot>> {
    if !valid_name(profile) {
        bail!("invalid profile identifier");
    }
    load(&open(path)?, profile)
}

/// A local operator explicitly edits one fixture slot. `expected` is the complete
/// previously returned version; `None` means the profile must not exist.
pub fn edit(
    path: &Path,
    profile: &str,
    expected: Option<&Version>,
    slot: usize,
    value: i32,
    source: &str,
) -> Result<Snapshot> {
    if !valid_name(profile) || !valid_name(source) {
        bail!("invalid profile or source identifier");
    }
    if slot >= 2 || !(-1000..=1000).contains(&value) {
        bail!("invalid fixture edit");
    }
    let mut db = open(path)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let old = load(&tx, profile)?;
    if old.as_ref().map(|s| &s.version) != expected {
        bail!("conflict: expected version is stale or belongs to another profile");
    }
    let mut values = old.as_ref().map_or([0, 0], |s| s.values);
    values[slot] = value;
    let bytes = serde_json::to_vec(&values)?;
    let digest = hash(&bytes);
    let generation = old.as_ref().map_or(1, |s| s.version.generation + 1);
    tx.execute("INSERT INTO snapshots(profile,generation,model,tokenizer,schema,state,digest,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![profile,generation,MODEL,TOKENIZER,SCHEMA,bytes,digest,source])?;
    let result = load(&tx, profile)?.context("published fixture state missing")?;
    tx.commit()?;
    Ok(result)
}
