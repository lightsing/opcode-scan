use crate::consts::{CONTRACT_TREE, INIT_CODE_TREE, TX_CONTRACT_ADDRESS_TREE};
use crate::db::*;
use crate::evm::Bytecode;
use ethers::prelude::*;
use sqlx::SqlitePool;

pub async fn listen_blocks(
    pool: SqlitePool,
    metadata: sled::Tree,
    provider: Provider<Ws>,
) -> anyhow::Result<()> {
    loop {
        let latest_recorded_block = get_latest_recorded_block(&metadata)?;
        let latest_block = provider.get_block_number().await?.as_u64();
        info!("Latest recorded block is #{}", latest_recorded_block);
        info!("Latest block is #{}", latest_block);
        if latest_recorded_block >= latest_block {
            break;
        }
        for block_number in (latest_recorded_block + 1)..=latest_block {
            submit_block_task(&pool, block_number).await?;
            set_latest_recorded_block(&metadata, block_number)?;
        }
    }

    info!("catch up done, listening for new blocks");
    let mut block_stream = provider.subscribe_blocks().await?;

    while let Some(block) = block_stream.next().await {
        let block_number = block.number.unwrap().as_u64();
        info!("new block #{} {}", block_number, block.hash.unwrap());
        submit_block_task(&pool, block_number).await?;
        set_latest_recorded_block(&metadata, block_number)?;
    }

    Ok(())
}

pub async fn handle_block(
    worker_id: usize,
    pool: SqlitePool,
    sled_db: sled::Db,
    provider: Provider<impl JsonRpcClient>,
) -> anyhow::Result<()> {
    let init_code_db = sled_db.open_tree(INIT_CODE_TREE)?;
    loop {
        let guard = BlockTaskGuard::new(&pool).await?;
        if guard.is_none() {
            // sleep
            info!(worker_id, "no block task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let guard = guard.unwrap();
        let block = provider
            .get_block_with_txs(guard.block_number())
            .await?
            .unwrap();
        assert_eq!(block.number.unwrap().as_u64(), guard.block_number());
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
            init_code_db.insert(tx.hash().as_bytes(), tx.input.as_ref())?;
            submit_tx_task(&pool, tx.hash()).await?;
            counter += 1;
        }
        if counter != 0 {
            info!(worker_id, "fetched {} create txs", counter);
        }
        guard.complete();
    }
}

pub async fn handle_tx(
    worker_id: usize,
    pool: SqlitePool,
    sled_db: sled::Db,
    provider: Provider<impl JsonRpcClient>,
) -> anyhow::Result<()> {
    let tx_contract_db = sled_db.open_tree(TX_CONTRACT_ADDRESS_TREE)?;
    let contract_db = sled_db.open_tree(CONTRACT_TREE)?;
    loop {
        let guard = TxTaskGuard::new(&pool).await?;
        if guard.is_none() {
            // sleep
            info!(worker_id, "no tx task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let guard = guard.unwrap();
        let tx_hash = guard.tx_hash();
        let tx = provider.get_transaction_receipt(tx_hash).await?.unwrap();
        if tx.status.unwrap().as_u64() == 0 {
            info!(worker_id, "skip failed tx {}", tx_hash);
            guard.complete();
            continue;
        }
        let contract_address = tx.contract_address.unwrap();
        info!(
            worker_id,
            "analyze tx {} deployed to contract {}", tx_hash, contract_address
        );
        let code = provider.get_code(contract_address, None).await?;
        if code.is_empty() {
            info!(worker_id, "skip empty contract {}", contract_address);
            guard.complete();
            continue;
        }
        tx_contract_db.insert(tx_hash.as_bytes(), contract_address.as_bytes())?;
        contract_db.insert(contract_address.as_bytes(), code.as_ref())?;
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
        guard.complete();
    }
}
