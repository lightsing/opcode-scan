#[macro_use]
extern crate tracing;
use crate::consts::{CODE_DB_PATH, HTTP_PROVIDER};
use crate::db::{clear_pending_tasks, init_sqlite};
use tracing_subscriber::EnvFilter;

mod consts;
mod db;
mod evm;
mod provider;
mod tasks;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::builder().from_env_lossy())
        .init();

    let pool = init_sqlite().await?;
    clear_pending_tasks(&pool).await?;
    let code_db = sled::open(CODE_DB_PATH)?;

    let listener = tokio::spawn(tasks::listen_blocks(
        pool.clone(),
        provider::ws_provider().await?,
    ));

    for (idx, key) in HTTP_PROVIDER.into_iter().enumerate() {
        for i in 0..10 {
            tokio::spawn(tasks::handle_block(
                idx * 10 + i,
                pool.clone(),
                code_db.clone(),
                provider::http_provider(key).await,
            ));
        }
        tokio::spawn(tasks::handle_tx(
            idx,
            pool.clone(),
            code_db.clone(),
            provider::http_provider(key).await,
        ));
    }

    listener.await??;
    Ok(())
}
