use {
    adrena_abi::{
        ADX_MINT, CORTEX_ID, GENESIS_LOCK_ID, MAIN_POOL_ID, SPL_ASSOCIATED_TOKEN_PROGRAM_ID,
        SPL_TOKEN_PROGRAM_ID, USDC_MINT,
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

    let args = adrena_abi::instruction::ClaimStakes {};
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
