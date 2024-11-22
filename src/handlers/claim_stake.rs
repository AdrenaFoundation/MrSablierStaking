use {
    crate::handlers::create_claim_stakes_ix,
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
        "  <*> Claiming stakes for UserStaking account {:#?} (owner: {:#?} staked token: {:#?})",
        user_staking_account_key,
        owner_pubkey,
        staked_token_mint
    );
    let transfer_authority_pda = get_transfer_authority_pda().0;
    let staking_pda = get_staking_pda(staked_token_mint).0;

    let staking_reward_token_vault_pda = get_staking_reward_token_vault_pda(&staking_pda).0;
    let staking_lm_reward_token_vault_pda = get_staking_lm_reward_token_vault_pda(&staking_pda).0;

    // First attempt to claim all stakes - if simulation fails, we will slowly reduce
    let mut remaining_indices: Vec<u8> = (0..32).collect();
    let mut postponed_indices: Vec<u8> = vec![];

    while !remaining_indices.is_empty() || !postponed_indices.is_empty() {
        let (claim_stakes_params, claim_stakes_accounts) = create_claim_stakes_ix(
            &program.payer(),
            owner_pubkey,
            transfer_authority_pda,
            &staking_pda,
            user_staking_account_key,
            &staking_reward_token_vault_pda,
            &staking_lm_reward_token_vault_pda,
            Some(&remaining_indices),
        );

        let rpc_client = program.rpc();

        let tx_simulation = program
            .request()
            .instruction(ComputeBudgetInstruction::set_compute_unit_price(
                median_priority_fee,
            ))
            .instruction(ComputeBudgetInstruction::set_compute_unit_limit(1_000_000))
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
                log::error!(
                    "   <> Simulation Transaction generation failed with error: {:?}",
                    e
                );
                backoff::Error::transient(e.into())
            })?;

        let mut simulation_attempts = 0;
        let simulation = loop {
            match rpc_client.simulate_transaction(&tx_simulation).await {
                Ok(simulation) => break simulation,
                Err(e) => {
                    if e.to_string().contains("BlockhashNotFound") {
                        simulation_attempts += 1;
                        log::warn!(
                            "   <> Simulation attempt {} failed with error: {:?} - Retrying...",
                            simulation_attempts,
                            e
                        );
                        if simulation_attempts >= 50 {
                            return Err(backoff::Error::transient(e.into()));
                        }
                    }
                    // If it's not a blockhash not found, we continue it's treated later
                }
            }
        };

        let simulated_cu = simulation.value.units_consumed.unwrap_or(0);
        // let simulation_logs = simulation.value.logs.unwrap_or(vec![]);
        // println!("   <> Simulation logs: {:?}", simulation_logs);

        if simulated_cu == 0 {
            log::warn!(
                "   <> CU consumed: {} - Seems that the simulation cannot be performed due to low sol balance OR that the state is not updated yet (postpone)",
                simulated_cu
            );
            break;
        }

        // If CU exceeds 1 million, reduce the number of indices (we use 500k instead of 1.4m cause it's more likely to land - eventually lower that further)
        if simulated_cu >= 1_000_000 {
            log::info!(
                "   <> CU consumed: {} - too high, postponing locked stake and retrying",
                simulated_cu
            );
            if let Some(index) = remaining_indices.pop() {
                postponed_indices.push(index);
            } else {
                log::warn!("   <> No more indices to pop from remaining_indices");
                return Err(backoff::Error::transient(anyhow::anyhow!(
                    "   <> No indices left to pop - Aborting"
                )));
            }
            continue;
        } else {
            log::info!("   <> CU consumed: {}", simulated_cu);
        }

        let (claim_stakes_params, claim_stakes_accounts) = create_claim_stakes_ix(
            &program.payer(),
            owner_pubkey,
            transfer_authority_pda,
            &staking_pda,
            user_staking_account_key,
            &staking_reward_token_vault_pda,
            &staking_lm_reward_token_vault_pda,
            Some(&remaining_indices),
        );

        let tx = program
            .request()
            .instruction(ComputeBudgetInstruction::set_compute_unit_price(
                median_priority_fee,
            ))
            .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
                (simulated_cu as f64 * 1.02) as u32, // +2% for any jitter due to find_pda calls
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
                log::error!("   <> Transaction generation failed with error: {:?}", e);
                backoff::Error::transient(e.into())
            })?;

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

        log::info!(
            "   <> Claim stakes for staking account {:#?} - TX sent: {:#?}",
            user_staking_account_key,
            tx_hash.to_string(),
        );

        // Reset remaining indices and move postponed indices to remaining
        remaining_indices = postponed_indices;
        postponed_indices = vec![];
    }

    Ok(())
}
