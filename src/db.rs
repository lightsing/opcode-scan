use crate::consts::{DB_PATH, SHANGHAI_FORK};
use ethers::prelude::*;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn init_sqlite() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query(
                    r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
            "#,
                )
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .max_connections(1)
        .connect(DB_PATH)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

pub async fn get_latest_recorded_block(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query!("SELECT block_number FROM block ORDER BY block_number DESC LIMIT 1")
            .fetch_optional(pool)
            .await?
            .map(|row| row.block_number as u64)
            .unwrap_or(SHANGHAI_FORK - 1),
    )
}

pub async fn append_block_task(pool: &SqlitePool, block_number: u64) -> Result<(), sqlx::Error> {
    let block_number = block_number as i64;
    sqlx::query!(
        "INSERT INTO block (block_number) VALUES (?) ON CONFLICT(block_number) DO NOTHING",
        block_number,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn append_tx_task(
    pool: &SqlitePool,
    block_number: u64,
    tx_index: u64,
    tx_hash: H256,
) -> Result<(), sqlx::Error> {
    let block_number = block_number as i64;
    let tx_index = tx_index as i64;
    let hash = tx_hash.as_ref();
    sqlx::query!(
        "INSERT INTO tx (block_number, tx_index, tx_hash) VALUES (?, ?, ?) ON CONFLICT(tx_hash) DO NOTHING",
        block_number,
        tx_index,
        hash,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn acquire_block_task(pool: &SqlitePool) -> Result<Option<u64>, sqlx::Error> {
    let block_number = sqlx::query!("SELECT block_number FROM block WHERE tx_fetched = 0 LIMIT 1",)
        .fetch_optional(pool)
        .await?
        .map(|row| row.block_number);
    if block_number.is_none() {
        return Ok(None);
    }
    let block_number = block_number.unwrap();
    sqlx::query!(
        "UPDATE block SET tx_fetched = 2 WHERE block_number = ?",
        block_number,
    )
    .execute(pool)
    .await?;
    Ok(Some(block_number as u64))
}

pub async fn mark_block_task_fetched(
    pool: &SqlitePool,
    block_number: u64,
) -> Result<(), sqlx::Error> {
    let block_number = block_number as i64;
    sqlx::query!(
        "UPDATE block SET tx_fetched = 1 WHERE block_number = ? AND tx_fetched = 2",
        block_number,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn acquire_tx_task(pool: &SqlitePool) -> Result<Option<H256>, sqlx::Error> {
    let tx_hash = sqlx::query!("SELECT tx_hash FROM tx WHERE tx_analyzed = 0 LIMIT 1",)
        .fetch_optional(pool)
        .await?
        .map(|row| row.tx_hash);
    if tx_hash.is_none() {
        return Ok(None);
    }
    let tx_hash = tx_hash.unwrap().unwrap();
    sqlx::query!("UPDATE tx SET tx_analyzed = 2 WHERE tx_hash = ?", tx_hash,)
        .execute(pool)
        .await?;
    Ok(Some(H256::from_slice(&tx_hash)))
}

// assert update row count == 1
pub async fn mark_tx_task_analyzed(pool: &SqlitePool, tx_hash: H256) -> Result<(), sqlx::Error> {
    let hash = tx_hash.as_ref();
    let result = sqlx::query!(
        "UPDATE tx SET tx_analyzed = 1 WHERE tx_hash = ? AND tx_analyzed = 2",
        hash,
    )
    .execute(pool)
    .await?;
    assert_eq!(result.rows_affected(), 1);
    Ok(())
}

pub async fn append_opcode_statistics(
    tx: &mut Transaction<'_, Sqlite>,
    block_number: u64,
    opcode: u8,
    count: u64,
) -> Result<(), sqlx::Error> {
    let block_number = block_number as i64;
    let opcode = opcode as i64;
    let count = count as i64;
    sqlx::query!(
        "INSERT INTO opcode_statistics (block_number, opcode, count) VALUES (?, ?, ?) ON CONFLICT(block_number, opcode) DO UPDATE SET count = count + ?",
        block_number,
        opcode,
        count,
        count,
    )
        .execute(tx.as_mut())
        .await?;
    Ok(())
}

pub async fn clear_pending_tasks(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE block SET tx_fetched = 0 WHERE tx_fetched = 2",)
        .execute(pool)
        .await?;
    sqlx::query!("UPDATE tx SET tx_analyzed = 0 WHERE tx_analyzed = 2",)
        .execute(pool)
        .await?;
    Ok(())
}
