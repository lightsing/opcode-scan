#[macro_use]
extern crate tracing;
use crate::consts::{HTTP_PROVIDER, METADATA_TREE, SLED_DB_PATH};
use crate::db::init_sqlite;
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
    let sled_db = sled::open(SLED_DB_PATH)?;

    let listener = tokio::spawn(tasks::listen_blocks(
        pool.clone(),
        sled_db.open_tree(METADATA_TREE)?,
        provider::ws_provider().await?,
    ));

    for i in 0..10 {
        tokio::spawn(tasks::handle_block(
            i,
            pool.clone(),
            sled_db.clone(),
            provider::http_provider(HTTP_PROVIDER).await,
        ));
    }
    tokio::spawn(tasks::handle_tx(
        1,
        pool.clone(),
        sled_db.clone(),
        provider::http_provider(HTTP_PROVIDER).await,
    ));

    listener.await??;
    Ok(())
}
