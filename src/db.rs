use crate::consts::{DB_PATH, LATEST_BLOCK_NUMBER, SHANGHAI_FORK};
use ethers::prelude::*;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

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
        .max_connections(5)
        .connect(DB_PATH)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

pub fn get_latest_recorded_block(tree: &sled::Tree) -> Result<u64, sled::Error> {
    tree.get(LATEST_BLOCK_NUMBER).map(|r| {
        r.and_then(|v| bincode::deserialize(&v).ok())
            .unwrap_or(SHANGHAI_FORK - 1)
    })
}

pub fn set_latest_recorded_block(tree: &sled::Tree, block_number: u64) -> Result<(), sled::Error> {
    tree.insert(
        LATEST_BLOCK_NUMBER,
        bincode::serialize(&block_number).unwrap(),
    )?;
    Ok(())
}

pub async fn submit_block_task(pool: &SqlitePool, block_number: u64) -> Result<(), sqlx::Error> {
    let block_number = block_number as i64;
    sqlx::query!(
        "INSERT INTO block_tasks (block_number) VALUES (?) ON CONFLICT(block_number) DO NOTHING",
        block_number,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn submit_tx_task(pool: &SqlitePool, tx_hash: H256) -> Result<(), sqlx::Error> {
    let hash = tx_hash.as_ref();
    sqlx::query!(
        "INSERT INTO tx_tasks (tx_hash) VALUES (?) ON CONFLICT(tx_hash) DO NOTHING",
        hash,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub struct BlockTaskGuard<'a> {
    pool: &'a SqlitePool,
    block_number: u64,
    finished: bool,
}

impl<'a> BlockTaskGuard<'a> {
    pub async fn new(pool: &'a SqlitePool) -> Result<Option<BlockTaskGuard<'a>>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"DELETE FROM block_tasks
            WHERE block_number = (
                SELECT block_number
                FROM block_tasks
                ORDER BY block_number ASC
                LIMIT 1
            )
            RETURNING block_number"#
        )
        .fetch_optional(pool)
        .await?
        .map(|r| Self {
            pool,
            block_number: r.block_number as u64,
            finished: false,
        }))
    }

    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    pub fn complete(mut self) {
        self.finished = true;
    }
}

impl<'a> Drop for BlockTaskGuard<'a> {
    fn drop(&mut self) {
        if !self.finished {
            let pool = self.pool.clone();
            let block_number = self.block_number;
            tokio::spawn(async move {
                if let Err(e) = submit_block_task(&pool, block_number).await {
                    error!("failed to re-submit block task: {}", e);
                }
            });
        }
    }
}

pub struct TxTaskGuard<'a> {
    pool: &'a SqlitePool,
    tx_hash: H256,
    finished: bool,
}

impl<'a> TxTaskGuard<'a> {
    pub async fn new(pool: &'a SqlitePool) -> Result<Option<TxTaskGuard<'a>>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"DELETE FROM tx_tasks
            WHERE tx_hash = (
                SELECT tx_hash
                FROM tx_tasks
                LIMIT 1
            )
            RETURNING tx_hash
            "#
        )
        .fetch_optional(pool)
        .await?
        .map(|r| Self {
            pool,
            tx_hash: H256::from_slice(&r.tx_hash),
            finished: false,
        }))
    }

    pub fn tx_hash(&self) -> H256 {
        self.tx_hash
    }

    pub fn complete(mut self) {
        self.finished = true;
    }
}

impl<'a> Drop for TxTaskGuard<'a> {
    fn drop(&mut self) {
        if !self.finished {
            let pool = self.pool.clone();
            let hash = self.tx_hash;
            tokio::spawn(async move {
                if let Err(e) = submit_tx_task(&pool, hash).await {
                    error!("failed to re-submit tx task: {}", e);
                }
            });
        }
    }
}

pub async fn append_opcode_statistics(
    pool: &SqlitePool,
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
        .execute(pool)
        .await?;
    Ok(())
}
