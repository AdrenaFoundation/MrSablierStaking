use {
    crate::{
        FinalizeLockedStakesCacheThreadSafe, IndexedStakingAccountsThreadSafe,
        IndexedUserStakingAccountsThreadSafe, StakingRoundNextResolveTimeCacheThreadSafe,
        UserStakingClaimCacheThreadSafe,
    },
    adrena_abi::{Pubkey, Staking, UserStaking, ROUND_MIN_DURATION_SECONDS},
    rand::{thread_rng, Rng},
    std::{cmp::min, collections::HashMap},
};

pub async fn update_staking_round_next_resolve_time_cache_for_account(
    staking_round_next_resolve_time_cache: &StakingRoundNextResolveTimeCacheThreadSafe,
    staking_account_key: &Pubkey,
    staking_account: &Staking,
) {
    let current_time = chrono::Utc::now().timestamp();
    // How long has the current staking round been running for?
    let round_current_duration = current_time - staking_account.current_staking_round.start_time;
    // If the current round has been running for longer than the minimum duration, set the next resolve time to be now
    let next_resolve_time = if round_current_duration >= ROUND_MIN_DURATION_SECONDS {
        current_time
    } else {
        staking_account.current_staking_round.start_time
            + ROUND_MIN_DURATION_SECONDS
            + thread_rng().gen_range(600..1800) // adding random + 10 to 60 min, to prevent staggering all claims together, and also for taking into account the slight delay in round execution
    };

    staking_round_next_resolve_time_cache
        .write()
        .await
        .insert(*staking_account_key, next_resolve_time);
}

pub async fn update_staking_round_next_resolve_time_cache(
    staking_round_next_resolve_time_cache: &StakingRoundNextResolveTimeCacheThreadSafe,
    indexed_staking_accounts: &IndexedStakingAccountsThreadSafe,
) {
    // For each Staking account, look at when the current staking round ends and set the next resolve time cache to be that time + the minimum round duration (6h)
    for (staking_account_key, staking_account) in indexed_staking_accounts.read().await.iter() {
        update_staking_round_next_resolve_time_cache_for_account(
            staking_round_next_resolve_time_cache,
            staking_account_key,
            staking_account,
        )
        .await;
    }
}

// Update the claim cache with the claim time of the oldest locked stake for each user staking account
pub async fn update_claim_cache(
    claim_cache: &UserStakingClaimCacheThreadSafe,
    indexed_user_staking_accounts: &IndexedUserStakingAccountsThreadSafe,
) {
    for (user_staking_account_key, user_staking_account) in
        indexed_user_staking_accounts.read().await.iter()
    {
        update_claim_cache_for_account(
            claim_cache,
            *user_staking_account_key,
            user_staking_account,
        )
        .await;
    }
}

/// Update the claim cache with the claim time of the oldest locked stake for a given UserStaking account
pub async fn update_claim_cache_for_account(
    claim_cache: &UserStakingClaimCacheThreadSafe,
    account_key: Pubkey,
    user_staking_account: &UserStaking,
) {
    let has_locked_stakes = user_staking_account
        .locked_stakes
        .iter()
        .any(|stake| stake.amount != 0);
    let has_liquid_stake = user_staking_account.liquid_stake.amount != 0;

    let oldest_claim_time_locked = if has_locked_stakes {
        user_staking_account
            .locked_stakes
            .iter()
            .filter(|stake| stake.amount != 0)
            .map(|stake| stake.claim_time)
            .min()
    } else {
        None
    };

    let oldest_claim_time_liquid = if has_liquid_stake {
        Some(user_staking_account.liquid_stake.claim_time)
    } else {
        None
    };

    let oldest_claim_time = match (oldest_claim_time_locked, oldest_claim_time_liquid) {
        (Some(locked), Some(liquid)) => Some(min(locked, liquid)),
        (Some(locked), None) => Some(locked),
        (None, Some(liquid)) => Some(liquid),
        (None, None) => None,
    };

    claim_cache
        .write()
        .await
        .insert(account_key, oldest_claim_time);
}

pub async fn update_finalize_locked_stakes_cache(
    finalize_locked_stakes_cache: &FinalizeLockedStakesCacheThreadSafe,
    indexed_user_staking_accounts: &IndexedUserStakingAccountsThreadSafe,
) {
    for (user_staking_account_key, user_staking_account) in
        indexed_user_staking_accounts.read().await.iter()
    {
        update_finalize_locked_stakes_cache_for_account(
            finalize_locked_stakes_cache,
            user_staking_account_key,
            user_staking_account,
        )
        .await;
    }
}

pub async fn update_finalize_locked_stakes_cache_for_account(
    finalize_locked_stakes_cache: &FinalizeLockedStakesCacheThreadSafe,
    user_staking_account_key: &Pubkey,
    user_staking_account: &UserStaking,
) {
    for ls in user_staking_account.locked_stakes.iter() {
        if ls.amount != 0 {
            finalize_locked_stakes_cache
                .write()
                .await
                .entry(*user_staking_account_key)
                .or_insert_with(HashMap::new)
                .insert(ls.id, ls.end_time);
        }
    }
}
