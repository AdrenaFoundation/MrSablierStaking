use {
    crate::{handlers::create_claim_stakes_ix, CLAIM_STAKES_CU_LIMIT},
    adrena_abi::{
        get_staking_lm_reward_token_vault_pda, get_staking_pda, get_staking_reward_token_vault_pda,
        get_transfer_authority_pda, ADX_MINT, SPL_TOKEN_PROGRAM_ID, USDC_MINT,
    },
    anchor_client::Program,
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{compute_budget::ComputeBudgetInstruction, pubkey::Pubkey, signature::Keypair},
    spl_associated_token_account::instruction::create_associated_token_account_idempotent,
    std::sync::Arc,
};

pub async fn claim_stakes(
    user_staking_account_key: &Pubkey,
    owner_pubkey: &Pubkey,
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
    staked_token_mint: &Pubkey,
) -> Result<(), backoff::Error<anyhow::Error>> {
    log::info!(
        "  <> Claiming stakes for UserStaking account {:#?} (owner: {:#?} staked token: {:#?})",
        user_staking_account_key,
        owner_pubkey,
        staked_token_mint
    );
    let transfer_authority_pda = get_transfer_authority_pda().0;
    let staking_pda = get_staking_pda(&staked_token_mint).0;

    let staking_reward_token_vault_pda = get_staking_reward_token_vault_pda(&staking_pda).0;
    let staking_lm_reward_token_vault_pda = get_staking_lm_reward_token_vault_pda(&staking_pda).0;

    let (claim_stakes_params, claim_stakes_accounts) = create_claim_stakes_ix(
        &program.payer(),
        owner_pubkey,
        transfer_authority_pda,
        &staking_pda,
        &user_staking_account_key,
        &staking_reward_token_vault_pda,
        &staking_lm_reward_token_vault_pda,
    );

    let tx = program
        .request()
        .instruction(ComputeBudgetInstruction::set_compute_unit_price(
            median_priority_fee,
        ))
        .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
            CLAIM_STAKES_CU_LIMIT,
        ))
        .instruction(create_associated_token_account_idempotent(
            &program.payer(),
            owner_pubkey,
            &ADX_MINT,
            &SPL_TOKEN_PROGRAM_ID,
        ))
        .instruction(create_associated_token_account_idempotent(
            &program.payer(),
            owner_pubkey,
            &USDC_MINT,
            &SPL_TOKEN_PROGRAM_ID,
        ))
        .args(claim_stakes_params)
        .accounts(claim_stakes_accounts)
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
        "  <> Claim stakes for staking account {:#?} - TX sent: {:#?}",
        user_staking_account_key,
        tx_hash.to_string(),
    );

    // TODO wait for confirmation and retry if needed

    Ok(())
}
