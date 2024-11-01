use {
    crate::{handlers::create_resolve_staking_round_ix, RESOLVE_STAKING_ROUND_CU_LIMIT},
    adrena_abi::get_transfer_authority_pda,
    anchor_client::Program,
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{compute_budget::ComputeBudgetInstruction, pubkey::Pubkey, signature::Keypair},
    std::sync::Arc,
};

pub async fn resolve_staking_round(
    staking_account_key: &Pubkey,
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
) -> Result<(), backoff::Error<anyhow::Error>> {
    log::info!(
        "  <> Resolving staking round for staking account {:#?}",
        staking_account_key
    );

    let transfer_authority_pda = get_transfer_authority_pda().0;
    let staking_staked_token_vault_pda =
        adrena_abi::pda::get_staking_staked_token_vault_pda(staking_account_key).0;
    let staking_reward_token_vault_pda =
        adrena_abi::pda::get_staking_reward_token_vault_pda(staking_account_key).0;
    let staking_lm_reward_token_vault_pda =
        adrena_abi::pda::get_staking_lm_reward_token_vault_pda(staking_account_key).0;

    let (resolve_staking_round_params, resolve_staking_round_accounts) =
        create_resolve_staking_round_ix(
            &program.payer(),
            transfer_authority_pda,
            staking_account_key,
            &staking_staked_token_vault_pda,
            &staking_reward_token_vault_pda,
            &staking_lm_reward_token_vault_pda,
        );

    let tx = program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(
            median_priority_fee,
        ))
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
            RESOLVE_STAKING_ROUND_CU_LIMIT,
        ))
        .args(resolve_staking_round_params)
        .accounts(resolve_staking_round_accounts)
        .signed_transaction()
        .await
        .map_err(|e| {
            log::error!("Transaction generation failed with error: {:?}", e);
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
            log::error!("Transaction sending failed with error: {:?}", e);
            backoff::Error::transient(e.into())
        })?;

    log::info!(
        "  <> Resolve staking round for staking account {:#?} - TX sent: {:#?}",
        staking_account_key,
        tx_hash.to_string(),
    );

    // TODO wait for confirmation and retry if needed

    Ok(())
}
