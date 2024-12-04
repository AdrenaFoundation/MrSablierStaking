use {
    crate::{handlers::create_update_pool_aum_ix, UPDATE_AUM_CU_LIMIT},
    anchor_client::Program,
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{
        compute_budget::ComputeBudgetInstruction, instruction::AccountMeta, signature::Keypair,
    },
    std::sync::Arc,
};

pub async fn update_pool_aum(
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
    remaining_accounts: Vec<AccountMeta>,
) -> Result<(), backoff::Error<anyhow::Error>> {
    log::info!("  <*> Updating AUM");

    let (update_pool_aum_params, update_pool_aum_accounts) =
        create_update_pool_aum_ix(&program.payer());

    let tx = program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(
            median_priority_fee,
        ))
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
            UPDATE_AUM_CU_LIMIT,
        ))
        .args(update_pool_aum_params)
        .accounts(update_pool_aum_accounts)
        // Remaining accounts
        .accounts(remaining_accounts)
        .signed_transaction()
        .await
        .map_err(|e| {
            log::error!("   <> Transaction generation failed with error: {:?}", e);
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
            log::error!("   <> Transaction sending failed with error: {:?}", e);
            backoff::Error::transient(e.into())
        })?;

    log::info!("   <> TX sent: {:#?}", tx_hash.to_string());

    Ok(())
}
