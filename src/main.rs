#[macro_use]
extern crate tracing;

use crate::consts::{HTTP_PROVIDER, METADATA_TREE, SLED_DB_PATH};
use crate::db::init_sqlite;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

    let running = Arc::new(AtomicBool::new(true));

    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        })?;
    }

    let pool = init_sqlite().await?;
    let sled_db = sled::open(SLED_DB_PATH)?;

    let mut join_handles = vec![];
    let listener = tokio::spawn(tasks::listen_blocks(
        pool.clone(),
        sled_db.open_tree(METADATA_TREE)?,
        provider::ws_provider().await?,
        running.clone(),
    ));
    join_handles.push(listener);

    for i in 0..1 {
        let worker = tokio::spawn(tasks::handle_block(
            i,
            pool.clone(),
            sled_db.clone(),
            provider::http_provider(HTTP_PROVIDER).await,
            running.clone(),
        ));
        join_handles.push(worker);
    }
    let worker = tokio::spawn(tasks::handle_tx(
        1,
        pool.clone(),
        sled_db.clone(),
        provider::http_provider(HTTP_PROVIDER).await,
        running.clone(),
    ));
    join_handles.push(worker);

    futures::future::join_all(join_handles).await;
    Ok(())
}
