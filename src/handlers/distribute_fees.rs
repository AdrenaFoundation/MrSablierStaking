use {
    crate::{
        get_last_trading_prices::get_last_trading_prices, handlers::create_distribute_fees_ix,
        IndexedCustodiesThreadSafe, DISTRIBUTE_FEES_CU_LIMIT,
    },
    adrena_abi::Cortex,
    anchor_client::Program,
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{
        compute_budget::ComputeBudgetInstruction, instruction::AccountMeta, signature::Keypair,
    },
    std::sync::Arc,
};

pub async fn distribute_fees(
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
    indexed_custodies: &IndexedCustodiesThreadSafe,
    cortex: &Cortex,
    remaining_accounts: Vec<AccountMeta>,
) -> Result<(), backoff::Error<anyhow::Error>> {
    log::info!("  <*> Distribute Fees");

    let last_trading_prices = get_last_trading_prices().await?;

    let (distribute_fees_params, distribute_fees_accounts) = create_distribute_fees_ix(
        &program.payer(),
        indexed_custodies,
        cortex.protocol_fee_recipient,
        Some(last_trading_prices),
    )
    .await;

    let tx = program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(
            median_priority_fee,
        ))
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
            DISTRIBUTE_FEES_CU_LIMIT,
        ))
        .args(distribute_fees_params)
        .accounts(distribute_fees_accounts)
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
