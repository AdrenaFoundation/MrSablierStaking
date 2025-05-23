use {
    crate::IndexedCustodiesThreadSafe,
    adrena_abi::{
        oracle::ChaosLabsBatchPrices, ADRENA_GOVERNANCE_REALM_CONFIG_ID,
        ADRENA_GOVERNANCE_REALM_ID, ADRENA_GOVERNANCE_SHADOW_TOKEN_MINT, ADX_MINT, ALP_MINT,
        CORTEX_ID, GENESIS_LOCK_ID, GOVERNANCE_PROGRAM_ID, MAIN_POOL_ID,
        SPL_ASSOCIATED_TOKEN_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, USDC_MINT,
    },
    solana_sdk::{pubkey::Pubkey, system_program},
};

pub fn create_resolve_staking_round_ix(
    payer: &Pubkey,
    transfer_authority_pda: Pubkey,
    staking_pda: &Pubkey,
    staking_staked_token_vault_pda: &Pubkey,
    staking_reward_token_vault_pda: &Pubkey,
    staking_lm_reward_token_vault_pda: &Pubkey,
) -> (
    adrena_abi::instruction::ResolveStakingRound,
    adrena_abi::accounts::ResolveStakingRound,
) {
    let args = adrena_abi::instruction::ResolveStakingRound {};
    let resolve_staking_round = adrena_abi::accounts::ResolveStakingRound {
        caller: *payer,
        payer: *payer,
        staking_staked_token_vault: *staking_staked_token_vault_pda,
        staking_reward_token_vault: *staking_reward_token_vault_pda,
        staking_lm_reward_token_vault: *staking_lm_reward_token_vault_pda,
        transfer_authority: transfer_authority_pda,
        staking: *staking_pda,
        cortex: CORTEX_ID,
        lm_token_mint: ADX_MINT,
        fee_redistribution_mint: USDC_MINT,
        adrena_program: adrena_abi::ID,
        system_program: system_program::ID,
        token_program: SPL_TOKEN_PROGRAM_ID,
    };
    let accounts = resolve_staking_round;
    (args, accounts)
}

pub fn create_claim_stakes_ix(
    payer: &Pubkey,
    owner_pubkey: &Pubkey,
    transfer_authority_pda: Pubkey,
    staking_pda: &Pubkey,
    user_staking_account_pda: &Pubkey,
    staking_reward_token_vault_pda: &Pubkey,
    staking_lm_reward_token_vault_pda: &Pubkey,
    locked_stake_indexes: Option<&Vec<u8>>,
) -> (
    adrena_abi::instruction::ClaimStakes,
    adrena_abi::accounts::ClaimStakes,
) {
    let reward_token_account = Pubkey::find_program_address(
        &[
            &owner_pubkey.to_bytes(),
            &SPL_TOKEN_PROGRAM_ID.to_bytes(),
            &USDC_MINT.to_bytes(),
        ],
        &SPL_ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0;
    let lm_token_account = Pubkey::find_program_address(
        &[
            &owner_pubkey.to_bytes(),
            &SPL_TOKEN_PROGRAM_ID.to_bytes(),
            &ADX_MINT.to_bytes(),
        ],
        &SPL_ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0;

    let args = adrena_abi::instruction::ClaimStakes {
        params: adrena_abi::types::ClaimStakesParams {
            locked_stake_indexes: locked_stake_indexes.cloned(),
        },
    };
    let accounts = adrena_abi::accounts::ClaimStakes {
        caller: *payer,
        payer: *payer,
        owner: *owner_pubkey,
        reward_token_account,
        lm_token_account,
        staking_reward_token_vault: *staking_reward_token_vault_pda,
        staking_lm_reward_token_vault: *staking_lm_reward_token_vault_pda,
        transfer_authority: transfer_authority_pda,
        user_staking: *user_staking_account_pda,
        staking: *staking_pda,
        cortex: CORTEX_ID,
        pool: MAIN_POOL_ID,
        genesis_lock: GENESIS_LOCK_ID,
        lm_token_mint: ADX_MINT,
        fee_redistribution_mint: USDC_MINT,
        adrena_program: adrena_abi::ID,
        system_program: system_program::ID,
        token_program: SPL_TOKEN_PROGRAM_ID,
    };
    (args, accounts)
}

pub fn create_finalize_locked_stake_ix(
    payer: &Pubkey,
    owner_pubkey: &Pubkey,
    locked_stake_id: u64,
    transfer_authority_pda: &Pubkey,
    staking_pda: &Pubkey,
    user_staking_account_pda: &Pubkey,
    governance_governing_token_holding_pda: &Pubkey,
    governance_governing_token_owner_record_pda: &Pubkey,
) -> (
    adrena_abi::instruction::FinalizeLockedStake,
    adrena_abi::accounts::FinalizeLockedStake,
) {
    let args = adrena_abi::instruction::FinalizeLockedStake {
        params: adrena_abi::types::FinalizeLockedStakeParams {
            early_exit: false,
            locked_stake_id,
        },
    };
    let finalize_locked_stake = adrena_abi::accounts::FinalizeLockedStake {
        caller: *payer,
        owner: *owner_pubkey,
        user_staking: *user_staking_account_pda,
        governance_token_mint: ADRENA_GOVERNANCE_SHADOW_TOKEN_MINT,
        governance_realm: ADRENA_GOVERNANCE_REALM_ID,
        governance_realm_config: ADRENA_GOVERNANCE_REALM_CONFIG_ID,
        governance_governing_token_holding: *governance_governing_token_holding_pda,
        governance_governing_token_owner_record: *governance_governing_token_owner_record_pda,
        transfer_authority: *transfer_authority_pda,
        staking: *staking_pda,
        cortex: CORTEX_ID,
        lm_token_mint: ADX_MINT,
        governance_program: GOVERNANCE_PROGRAM_ID,
        adrena_program: adrena_abi::ID,
        system_program: system_program::ID,
        token_program: SPL_TOKEN_PROGRAM_ID,
    };
    let accounts = finalize_locked_stake;
    (args, accounts)
}

pub fn create_update_pool_aum_ix(
    payer: &Pubkey,
    last_trading_prices: Option<ChaosLabsBatchPrices>,
) -> (
    adrena_abi::instruction::UpdatePoolAum,
    adrena_abi::accounts::UpdatePoolAum,
) {
    let oracle_pda = adrena_abi::pda::get_oracle_pda().0;

    let args = adrena_abi::instruction::UpdatePoolAum {
        params: adrena_abi::types::UpdatePoolAumParams {
            oracle_prices: last_trading_prices,
        },
    };

    let accounts = adrena_abi::accounts::UpdatePoolAum {
        payer: *payer,
        cortex: CORTEX_ID,
        pool: MAIN_POOL_ID,
        oracle: oracle_pda,
    };

    (args, accounts)
}

pub async fn create_distribute_fees_ix(
    payer: &Pubkey,
    indexed_custodies: &IndexedCustodiesThreadSafe,
    protocol_fee_recipient: Pubkey,
    last_trading_prices: Option<ChaosLabsBatchPrices>,
) -> (
    adrena_abi::instruction::DistributeFees,
    adrena_abi::accounts::DistributeFees,
) {
    let transfer_authority_pda = adrena_abi::pda::get_transfer_authority_pda().0;
    let lm_staking = adrena_abi::pda::get_staking_pda(&ADX_MINT).0;
    let lp_staking = adrena_abi::pda::get_staking_pda(&ALP_MINT).0;
    let oracle_pda = adrena_abi::pda::get_oracle_pda().0;

    let usdc_custody_pubkey = indexed_custodies
        .read()
        .await
        .iter()
        .find_map(|(pubkey, custody)| {
            if custody.mint == USDC_MINT {
                Some(*pubkey)
            } else {
                None
            }
        })
        .unwrap();

    let usdc_custody_token_account = indexed_custodies
        .read()
        .await
        .get(&usdc_custody_pubkey)
        .unwrap()
        .token_account;

    let args = adrena_abi::instruction::DistributeFees {
        params: adrena_abi::types::DistributeFeesParams {
            oracle_prices: last_trading_prices,
        },
    };
    let accounts = adrena_abi::accounts::DistributeFees {
        caller: *payer,
        transfer_authority: transfer_authority_pda,
        cortex: CORTEX_ID,
        pool: MAIN_POOL_ID,
        lm_staking,
        lp_staking,
        lm_token_mint: ADX_MINT,
        lp_token_mint: ALP_MINT,
        fee_redistribution_mint: USDC_MINT,
        lm_staking_reward_token_vault: adrena_abi::pda::get_staking_reward_token_vault_pda(
            &lm_staking,
        )
        .0,
        lp_staking_reward_token_vault: adrena_abi::pda::get_staking_reward_token_vault_pda(
            &lp_staking,
        )
        .0,
        referrer_reward_token_vault: adrena_abi::pda::get_referrer_reward_token_vault_pda(
            &USDC_MINT,
        )
        .0,
        staking_reward_token_custody: usdc_custody_pubkey,
        staking_reward_token_custody_token_account: usdc_custody_token_account,
        oracle: oracle_pda,
        protocol_fee_recipient,
        token_program: SPL_TOKEN_PROGRAM_ID,
        system_program: system_program::ID,
        adrena_program: adrena_abi::ID,
    };
    (args, accounts)
}
