use crate::db::*;
use crate::evm::Bytecode;
use ethers::prelude::*;
use sqlx::SqlitePool;

pub async fn listen_blocks(pool: SqlitePool, provider: Provider<Ws>) -> anyhow::Result<()> {
    // catch up
    loop {
        let latest_recorded_block = get_latest_recorded_block(&pool).await?;
        let latest_block = provider.get_block_number().await?.as_u64();
        info!("Latest recorded block is #{}", latest_recorded_block);
        info!("Latest block is #{}", latest_block);
        if latest_recorded_block >= latest_block {
            break;
        }
        for block_number in (latest_recorded_block + 1)..=latest_block {
            append_block_task(&pool, block_number).await?;
        }
    }

    info!("catch up done, listening for new blocks");
    let mut block_stream = provider.subscribe_blocks().await?;

    while let Some(block) = block_stream.next().await {
        info!(
            "new block #{} {}",
            block.number.unwrap().as_u64(),
            block.hash.unwrap()
        );
        append_block_task(&pool, block.number.unwrap().as_u64()).await?;
    }

    Ok(())
}

pub async fn handle_block(
    worker_id: usize,
    pool: SqlitePool,
    provider: Provider<impl JsonRpcClient>,
) -> anyhow::Result<()> {
    loop {
        let block_number = acquire_block_task(&pool).await?;
        if block_number.is_none() {
            // sleep
            info!(worker_id, "no block task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let block_number = block_number.unwrap();
        let block = provider.get_block_with_txs(block_number).await?.unwrap();
        info!(
            worker_id,
            "fetching block #{} {}",
            block.number.unwrap().as_u64(),
            block.hash.unwrap()
        );
        let mut counter = 0;
        for tx in block.transactions.iter() {
            if tx.to.is_some() {
                continue;
            }
            append_tx_task(
                &pool,
                block_number,
                tx.transaction_index.unwrap().as_u64(),
                tx.hash(),
            )
            .await?;
            counter += 1;
        }
        if counter != 0 {
            info!(worker_id, "fetched {} create txs", counter);
        }
        mark_block_task_fetched(&pool, block_number).await?;
    }
}

pub async fn handle_tx(
    pool: SqlitePool,
    provider: Provider<impl JsonRpcClient>,
) -> anyhow::Result<()> {
    loop {
        let tx_hash = acquire_tx_task(&pool).await?;
        if tx_hash.is_none() {
            // sleep
            info!("no tx task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let tx_hash = tx_hash.unwrap();
        let tx = provider.get_transaction_receipt(tx_hash).await?.unwrap();
        if tx.status.unwrap().as_u64() == 0 {
            info!("skip failed tx {}", tx_hash);
            mark_tx_task_analyzed(&pool, tx_hash).await?;
            continue;
        }
        let contract_address = tx.contract_address.unwrap();
        info!(
            "analyze tx {} deployed to contract {}",
            tx_hash, contract_address
        );
        let code = provider.get_code(contract_address, None).await?;
        let ops = Bytecode::from(code.to_vec());
        let count = ops
            .code
            .into_iter()
            .filter(|op| op.is_code)
            .map(|op| op.value)
            .fold([0usize; 256], |mut acc, x| {
                acc[x as usize] += 1;
                acc
            })
            .into_iter()
            .enumerate()
            .filter(|(_, count)| *count > 0);
        for (opcode, count) in count {
            append_opcode_statistics(
                &pool,
                tx.block_number.unwrap().as_u64(),
                opcode as u8,
                count as u64,
            )
            .await?;
        }
        mark_tx_task_analyzed(&pool, tx_hash).await?;
    }
}
