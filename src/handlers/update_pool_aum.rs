use {
    crate::{
        handlers::create_update_pool_aum_ix, IndexedCustodiesThreadSafe,
        RESOLVE_STAKING_ROUND_CU_LIMIT,
    },
    adrena_abi::AccountMeta,
    anchor_client::Program,
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{compute_budget::ComputeBudgetInstruction, pubkey::Pubkey, signature::Keypair},
    std::sync::Arc,
};

pub async fn update_pool_aum(
    pool_id: Pubkey,
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
    custodies: &IndexedCustodiesThreadSafe,
) -> Result<(), backoff::Error<anyhow::Error>> {
    log::info!("  <*> Updating pool AUM for pool {:#?}", pool_id);

    let (update_pool_aum_params, update_pool_aum_accounts, remaining_accounts) =
        create_update_pool_aum_ix(&program.payer(), pool_id, custodies).await;

    let tx = program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(
            median_priority_fee,
        ))
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
            RESOLVE_STAKING_ROUND_CU_LIMIT,
        ))
        .args(update_pool_aum_params)
        .accounts(update_pool_aum_accounts)
        // Remaining accounts
        .accounts(
            remaining_accounts
                .iter()
                .map(|account| AccountMeta {
                    pubkey: *account,
                    is_signer: false,
                    is_writable: false,
                })
                .collect::<Vec<AccountMeta>>(),
        )
        .signed_transaction()
        .await
        .map_err(|e| {
            log::error!("  <> Transaction generation failed with error: {:?}", e);
            backoff::Error::transient(e.into())
        })?;

    let rpc_client = program.rpc();

    let tx_hash = rpc_client
        .send_transaction_with_config(
            &tx,
            RpcSendTransactionConfig {
                skip_preflight: true,
                max_retries: Some(0),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| {
            log::error!("  <> Transaction sending failed with error: {:?}", e);
            backoff::Error::transient(e.into())
        })?;

    log::info!(
        "  <> Update pool AUM for pool {:#?} - TX sent: {:#?}",
        pool_id,
        tx_hash.to_string(),
    );

    // TODO wait for confirmation and retry if needed

    Ok(())
}
