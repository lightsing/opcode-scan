use crate::consts::{CONTRACT_TREE, INIT_CODE_TREE, TX_CONTRACT_ADDRESS_TREE};
use crate::db::*;
use crate::evm::{Bytecode, OpcodeId};
use ethers::prelude::*;
use sqlx::SqlitePool;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[instrument(skip_all)]
pub async fn listen_blocks(
    pool: SqlitePool,
    metadata: sled::Tree,
    provider: Provider<Ws>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    while running.load(std::sync::atomic::Ordering::SeqCst) {
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
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let block_number = block.number.unwrap().as_u64();
        info!("new block #{} {}", block_number, block.hash.unwrap());
        submit_block_task(&pool, block_number).await?;
        set_latest_recorded_block(&metadata, block_number)?;
    }

    Ok(())
}

#[instrument(skip_all, fields(worker_id = %worker_id))]
pub async fn handle_block(
    worker_id: usize,
    pool: SqlitePool,
    sled_db: sled::Db,
    provider: Provider<impl JsonRpcClient>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let init_code_db = sled_db.open_tree(INIT_CODE_TREE)?;
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let guard = BlockTaskGuard::new(&pool).await?;
        if guard.is_none() {
            // sleep
            info!("no block task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let guard = guard.unwrap();
        let block = provider
            .get_block_with_txs(guard.block_number())
            .await?
            .unwrap();
        assert_eq!(block.number.unwrap().as_u64(), guard.block_number());
        trace!(
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
            trace!("fetched {} create txs", counter);
        }
        guard.complete();
    }
    info!("gracefully shutdown");
    Ok(())
}

#[instrument(skip_all, fields(worker_id = %worker_id))]
pub async fn handle_tx(
    worker_id: usize,
    pool: SqlitePool,
    sled_db: sled::Db,
    provider: Provider<impl JsonRpcClient>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let tx_contract_db = sled_db.open_tree(TX_CONTRACT_ADDRESS_TREE)?;
    let contract_db = sled_db.open_tree(CONTRACT_TREE)?;
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let guard = TxTaskGuard::new(&pool).await?;
        if guard.is_none() {
            // sleep
            info!("no tx task, sleep");
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            continue;
        }
        let guard = guard.unwrap();
        let tx_hash = guard.tx_hash();
        let tx = provider.get_transaction_receipt(tx_hash).await?.unwrap();
        if tx.status.unwrap().as_u64() == 0 {
            trace!("skip failed tx {}", tx_hash);
            guard.complete();
            continue;
        }
        let contract_address = tx.contract_address.unwrap();
        trace!(
            "analyze tx {} deployed to contract {}",
            tx_hash,
            contract_address
        );
        let code = provider.get_code(contract_address, None).await?;
        if code.is_empty() {
            trace!("skip empty contract {}", contract_address);
            guard.complete();
            continue;
        }
        tx_contract_db.insert(tx_hash.as_bytes(), contract_address.as_bytes())?;
        contract_db.insert(contract_address.as_bytes(), code.as_ref())?;
        let ops = Bytecode::from(code.to_vec());
        let opcodes = ops
            .code
            .into_iter()
            .filter(|op| op.is_code)
            .map(|op| OpcodeId::from(op.value))
            .collect::<Vec<_>>();
        if opcodes.iter().any(|opcode| opcode.is_other_invalid()) {
            warn!("contract {:?} contains invalid opcodes", contract_address,);
        }

        let count = opcodes
            .iter()
            .fold([0usize; 256], |mut acc, x| {
                acc[x.as_u8() as usize] += 1;
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
    info!("gracefully shutdown");
    Ok(())
}
