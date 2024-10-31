// use {
//     crate::{handlers::create_close_position_long_ix, CLOSE_POSITION_LONG_CU_LIMIT},
//     adrena_abi::{
//         get_sablier_thread_pda, get_transfer_authority_pda, get_user_profile_pda,
//         main_pool::USDC_CUSTODY_ID, types::Cortex, Position, ADX_MINT, ALP_MINT,
//         SPL_ASSOCIATED_TOKEN_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID,
//     },
//     anchor_client::Client,
//     solana_client::rpc_config::RpcSendTransactionConfig,
//     solana_sdk::{compute_budget::ComputeBudgetInstruction, pubkey::Pubkey, signature::Keypair},
//     std::sync::Arc,
// };

// pub async fn claim_stakes(
//     user_staking_account_key: &Pubkey,
//     client: &Client<Arc<Keypair>>,
//     median_priority_fee: u64,
// ) -> Result<(), backoff::Error<anyhow::Error>> {
//     let program = client
//         .program(adrena_abi::ID)
//         .map_err(|e| backoff::Error::transient(e.into()))?;

//     let indexed_user_staking_accounts = indexed_user_staking_accounts.read().await;
//     let user_staking_account = indexed_user_staking_accounts.get(user_staking_account_key).unwrap();
//     let custody = user_staking_account.custody;
//     let collateral_custody = user_staking_account.collateral_custody;
//         .get(&position.collateral_custody)
//         .unwrap();
//     let staking_reward_token_custody = indexed_custodies_read.get(&USDC_CUSTODY_ID).unwrap();

//     let collateral_mint = collateral_custody.mint;

//     let receiving_account = Pubkey::find_program_address(
//         &[
//             &position.owner.to_bytes(),
//             &SPL_TOKEN_PROGRAM_ID.to_bytes(),
//             &collateral_mint.to_bytes(),
//         ],
//         &SPL_ASSOCIATED_TOKEN_PROGRAM_ID,
//     )
//     .0;

//     let user_profile_pda = get_user_profile_pda(&position.owner).0;
//     // Fetch the user profile account
//     let user_profile_account = program.rpc().get_account(&user_profile_pda).await.ok(); // Convert Result to Option, None if error

//     // Check if the user profile exists (owned by the Adrena program)
//     let user_profile = match user_profile_account {
//         Some(account) if account.owner == adrena_abi::ID => Some(user_profile_pda),
//         _ => None,
//     };

//     let transfer_authority_pda = get_transfer_authority_pda().0;

//     let position_take_profit_pda = get_sablier_thread_pda(
//         &transfer_authority_pda,
//         position.take_profit_thread_id.to_le_bytes().to_vec(),
//         Some(position.owner.to_bytes().to_vec()),
//     )
//     .0;

//     let position_stop_loss_pda = get_sablier_thread_pda(
//         &transfer_authority_pda,
//         position.stop_loss_thread_id.to_le_bytes().to_vec(),
//         Some(position.owner.to_bytes().to_vec()),
//     )
//     .0;

//     let lm_staking = adrena_abi::pda::get_staking_pda(&ADX_MINT).0;
//     let lp_staking = adrena_abi::pda::get_staking_pda(&ALP_MINT).0;

//     let (close_position_long_params, close_position_long_accounts) = create_close_position_long_ix(
//         &program.payer(),
//         position_key,
//         position,
//         receiving_account,
//         transfer_authority_pda,
//         lm_staking,
//         lp_staking,
//         cortex,
//         user_profile,
//         position_take_profit_pda,
//         position_stop_loss_pda,
//         staking_reward_token_custody,
//         custody,
//         position.stop_loss_close_position_price,
//     );

//     let tx = program
//         .request()
//         .instruction(ComputeBudgetInstruction::set_compute_unit_price(
//             median_priority_fee,
//         ))
//         .instruction(ComputeBudgetInstruction::set_compute_unit_limit(
//             CLOSE_POSITION_LONG_CU_LIMIT,
//         ))
//         .args(close_position_long_params)
//         .accounts(close_position_long_accounts)
//         .signed_transaction()
//         .await
//         .map_err(|e| {
//             log::error!("Transaction generation failed with error: {:?}", e);
//             backoff::Error::transient(e.into())
//         })?;

//     let rpc_client = program.rpc();

//     let tx_hash = rpc_client
//         .send_transaction_with_config(
//             &tx,
//             RpcSendTransactionConfig {
//                 skip_preflight: true,
//                 max_retries: Some(0),
//                 ..Default::default()
//             },
//         )
//         .await
//         .map_err(|e| {
//             log::error!("Transaction sending failed with error: {:?}", e);
//             backoff::Error::transient(e.into())
//         })?;

//     log::info!(
//         "SL Long for position {:#?} - TX sent: {:#?}",
//         position_key,
//         tx_hash.to_string(),
//     );

//     // TODO wait for confirmation and retry if needed

//     Ok(())
// }
