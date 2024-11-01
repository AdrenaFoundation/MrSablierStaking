use {
    crate::{process_stream_message::process_stream_message, update_caches::update_claim_cache},
    adrena_abi::{Staking, UserStaking, ROUND_MIN_DURATION_SECONDS},
    anchor_client::{solana_sdk::signer::keypair::read_keypair_file, Client, Cluster, Program},
    backoff::{future::retry, ExponentialBackoff},
    clap::Parser,
    futures::{StreamExt, TryFutureExt},
    priority_fees::fetch_mean_priority_fee,
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::{pubkey::Pubkey, signature::Keypair},
    std::{collections::HashMap, env, str::FromStr, sync::Arc, time::Duration},
    tokio::{
        sync::{Mutex, RwLock},
        task::JoinHandle,
        time::interval,
    },
    tonic::transport::channel::ClientTlsConfig,
    update_caches::update_staking_round_next_resolve_time_cache,
    yellowstone_grpc_client::{GeyserGrpcClient, Interceptor},
    yellowstone_grpc_proto::{
        geyser::{
            SubscribeRequest, SubscribeRequestFilterAccountsFilter,
            SubscribeRequestFilterAccountsFilterMemcmp,
        },
        prelude::{
            subscribe_request_filter_accounts_filter::Filter as AccountsFilterDataOneof,
            subscribe_request_filter_accounts_filter_memcmp::Data as AccountsFilterMemcmpOneof,
            CommitmentLevel, SubscribeRequestFilterAccounts,
        },
    },
};use adrena_abi::{StakingType, ADX_MINT, ALP_MINT};

use openssl::ssl::{SslConnector, SslMethod};
use postgres_openssl::MakeTlsConnector;

type AccountFilterMap = HashMap<String, SubscribeRequestFilterAccounts>;

type IndexedStakingAccountsThreadSafe = Arc<RwLock<HashMap<Pubkey, Staking>>>;
type IndexedUserStakingAccountsThreadSafe = Arc<RwLock<HashMap<Pubkey, UserStaking>>>;
// Cache the claim time of the oldest locked stake for each user staking account - This is used to determine when we should trigger the next auto claim
// If none, no auto claim is needed
type UserStakingClaimCacheThreadSafe = Arc<RwLock<HashMap<Pubkey, Option<i64>>>>;
// Cache the time of next execution for the resolve staking round task, keyed by Staking account pda
type StakingRoundNextResolveTimeCacheThreadSafe = Arc<RwLock<HashMap<Pubkey, i64>>>;

pub mod handlers;
pub mod priority_fees;
pub mod process_stream_message;
pub mod update_caches;
pub mod update_indexes;
pub mod utils;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:10000";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MEAN_PRIORITY_FEE_PERCENTILE: u64 = 5000; // 50th
const PRIORITY_FEE_REFRESH_INTERVAL: Duration = Duration::from_secs(5); 
pub const CLOSE_POSITION_LONG_CU_LIMIT: u32 = 380_000;
pub const CLOSE_POSITION_SHORT_CU_LIMIT: u32 = 280_000;
pub const CLEANUP_POSITION_CU_LIMIT: u32 = 60_000;
pub const LIQUIDATE_LONG_CU_LIMIT: u32 = 310_000;
pub const LIQUIDATE_SHORT_CU_LIMIT: u32 = 210_000;
pub const RESOLVE_STAKING_ROUND_CU_LIMIT: u32 = 400_000;
pub const CLAIM_STAKES_CU_LIMIT: u32 = 400_000;

// The threshold to trigger a claim of the stakes for a UserStaking account - we can store up to 32 rounds data per account, we do so to avoid loosing rewards
pub const AUTO_CLAIM_THRESHOLD_SECONDS: i64 = ROUND_MIN_DURATION_SECONDS * 25; 

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum ArgsCommitment {
    #[default]
    Processed,
    Confirmed,
    Finalized,
}

impl From<ArgsCommitment> for CommitmentLevel {
    fn from(commitment: ArgsCommitment) -> Self {
        match commitment {
            ArgsCommitment::Processed => CommitmentLevel::Processed,
            ArgsCommitment::Confirmed => CommitmentLevel::Confirmed,
            ArgsCommitment::Finalized => CommitmentLevel::Finalized,
        }
    }
}

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long, default_value_t = String::from(DEFAULT_ENDPOINT))]
    /// Service endpoint
    endpoint: String,

    #[clap(long)]
    x_token: Option<String>,

    /// Commitment level: processed, confirmed or finalized
    #[clap(long)]
    commitment: Option<ArgsCommitment>,

    /// Path to the payer keypair
    #[clap(long)]
    payer_keypair: String,

    /// DB Url
    #[clap(long)]
    db_string: String,
}

impl Args {
    fn get_commitment(&self) -> Option<CommitmentLevel> {
        Some(self.commitment.unwrap_or_default().into())
    }

    async fn connect(&self) -> anyhow::Result<GeyserGrpcClient<impl Interceptor>> {
        GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token(self.x_token.clone())?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .connect()
            .await
            .map_err(Into::into)
    }
}

pub fn get_staking_anchor_discriminator() -> Vec<u8> {
    utils::derive_discriminator("Staking").to_vec()
}

pub fn get_user_staking_anchor_discriminator() -> Vec<u8> {
    utils::derive_discriminator("UserStaking").to_vec()
}

async fn generate_accounts_filter_map(
    indexed_user_staking_accounts: &IndexedUserStakingAccountsThreadSafe,
) -> AccountFilterMap {
    // Create the accounts filter map (on all Staking and UserStaking accounts based on discriminator)
    let mut accounts_filter_map: AccountFilterMap = HashMap::new();

    // Staking accounts (goal it to catch updates to the current staking round end time for resolving the just previous staking round)
    let staking_filter_discriminator = SubscribeRequestFilterAccountsFilter {
        filter: Some(AccountsFilterDataOneof::Memcmp(
            SubscribeRequestFilterAccountsFilterMemcmp {
                offset: 0,
                data: Some(AccountsFilterMemcmpOneof::Bytes(
                    get_staking_anchor_discriminator(),
                )),
            },
        )),
    };
    let staking_owner = vec![adrena_abi::ID.to_string()];
    accounts_filter_map.insert(
        "staking_create_update".to_owned(),
        SubscribeRequestFilterAccounts {
            account: vec![],
            owner: staking_owner,
            filters: vec![staking_filter_discriminator],
        },
    );
    // We don't monitor Staking accounts for close events - These are ever lasting accounts

    // Retrieve the existing user staking accounts keys - they are monitored for close events
    let existing_user_staking_accounts_keys: Vec<String> = indexed_user_staking_accounts
        .read()
        .await
        .keys()
        .map(|p| p.to_string())
        .collect();
    // User staking accounts (will catch new user staking accounts created and modified user staking accounts)
    let user_staking_filter_discriminator = SubscribeRequestFilterAccountsFilter {
        filter: Some(AccountsFilterDataOneof::Memcmp(
            SubscribeRequestFilterAccountsFilterMemcmp {
                offset: 0,
                data: Some(AccountsFilterMemcmpOneof::Bytes(
                    get_user_staking_anchor_discriminator(),
                )),
            },
        )),
    };
    let user_staking_owner = vec![adrena_abi::ID.to_string()];
    accounts_filter_map.insert(
        "user_staking_create_update".to_owned(),
        SubscribeRequestFilterAccounts {
            account: vec![],
            owner: user_staking_owner,
            filters: vec![user_staking_filter_discriminator],
        },
    );

    // Existing user staking accounts - We monitor these to catch when they are closed
    accounts_filter_map.insert(
        "user_staking_close".to_owned(),
        SubscribeRequestFilterAccounts {
            account: existing_user_staking_accounts_keys,
            owner: vec![],
            filters: vec![],
        },
    );

    accounts_filter_map
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env::set_var(
        env_logger::DEFAULT_FILTER_ENV,
        env::var_os(env_logger::DEFAULT_FILTER_ENV).unwrap_or_else(|| "info".into()),
    );
    env_logger::init();

    let args = Args::parse();
    let zero_attempts = Arc::new(Mutex::new(true));

    // The array of indexed Staking accounts (these are the top level ADX and ALP staking "config" accounts)
    let indexed_staking_accounts: IndexedStakingAccountsThreadSafe =
        Arc::new(RwLock::new(HashMap::new()));
    // The array of indexed Locked Staking accounts (these are the users locked stakes, mixing ADX and ALP)
    let indexed_user_staking_accounts: IndexedUserStakingAccountsThreadSafe =
        Arc::new(RwLock::new(HashMap::new()));
    let claim_cache: UserStakingClaimCacheThreadSafe = Arc::new(RwLock::new(HashMap::new()));
    let staking_round_next_resolve_time_cache: StakingRoundNextResolveTimeCacheThreadSafe =
        Arc::new(RwLock::new(HashMap::new()));

    // The default exponential backoff strategy intervals:
    // [500ms, 750ms, 1.125s, 1.6875s, 2.53125s, 3.796875s, 5.6953125s,
    // 8.5s, 12.8s, 19.2s, 28.8s, 43.2s, 64.8s, 97s, ... ]
    retry(ExponentialBackoff::default(), move || {
        let args = args.clone();
        let zero_attempts = Arc::clone(&zero_attempts);
        let indexed_staking_accounts = Arc::clone(&indexed_staking_accounts);
        let indexed_user_staking_accounts = Arc::clone(&indexed_user_staking_accounts);
        let claim_cache = Arc::clone(&claim_cache);
        let staking_round_next_resolve_time_cache = Arc::clone(&staking_round_next_resolve_time_cache);
        let mut periodical_priority_fees_fetching_task: Option<JoinHandle<Result<(), backoff::Error<anyhow::Error>>>> = None;
        let mut periodical_claim_stakes_task: Option<JoinHandle<Result<(), backoff::Error<anyhow::Error>>>> = None;
        let mut periodical_resolve_staking_rounds_task: Option<JoinHandle<Result<(), backoff::Error<anyhow::Error>>>> = None;
        let mut db_connection_task: Option<JoinHandle<()>> = None;

        async move {
            // In case it errored out, abort the fee task (will be recreated)
            if let Some(t) = periodical_priority_fees_fetching_task.take() {
                t.abort();
            }
            if let Some(t) = periodical_claim_stakes_task.take() {
                t.abort();
            }
            if let Some(t) = periodical_resolve_staking_rounds_task.take() {
                t.abort();
            }
            if let Some(t) = db_connection_task.take() {
                t.abort();
            }

            let mut zero_attempts = zero_attempts.lock().await;
            if *zero_attempts {
                *zero_attempts = false;
            } else {
                log::info!("Retry to connect to the server");
            }
            drop(zero_attempts);

            let commitment = args.get_commitment();
            let mut grpc = args
                .connect()
                .await
                .map_err(backoff::Error::transient)?;

            let payer = read_keypair_file(args.payer_keypair.clone()).unwrap();
            let payer = Arc::new(payer);
            let client = Client::new(
                Cluster::Custom(args.endpoint.clone(), args.endpoint.clone()),
                Arc::clone(&payer),
            );
            let program = client
                .program(adrena_abi::ID)
                .map_err(|e| backoff::Error::transient(e.into()))?;
            log::info!("  <> gRPC, RPC clients connected!");

            // Connect to the DB that contains the table matching the UserStaking accounts to their owners (the onchain data doesn't contain the owner)
            // Create an SSL connector
            let builder = SslConnector::builder(SslMethod::tls()).unwrap();
            let connector = MakeTlsConnector::new(builder.build());
            let (db, db_connection) = tokio_postgres::connect(&args.db_string, connector).await.map_err(|e| backoff::Error::transient(e.into()))?;
            // Open a connection to the DB
            #[allow(unused_assignments)]
            {
                db_connection_task = Some(tokio::spawn(async move {
                    if let Err(e) = db_connection.await {
                        log::error!("connection error: {}", e);
                    }
                }));
            }

            // ////////////////////////////////////////////////////////////////
            log::info!("1 - Retrieving and indexing all Staking andUserStaking accounts...");
            let existing_user_staking_accounts_without_staking_type = {
                // Staking accounts
                {
                let staking_pda_filter = RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                    0,
                    &get_staking_anchor_discriminator(),
                ));
                let filters = vec![staking_pda_filter];
                let existing_staking_accounts = program
                    .accounts::<Staking>(filters)
                    .await
                    .map_err(|e| backoff::Error::transient(e.into()))?;
                {
                    let mut indexed_staking_accounts = indexed_staking_accounts.write().await;

                    indexed_staking_accounts.extend(existing_staking_accounts);
                }
                log::info!(
                    "  <> # of existing Staking accounts parsed and loaded: {}",
                    indexed_staking_accounts.read().await.len()
                );
                }

                // User staking accounts
                let existing_user_staking_accounts_without_staking_type = {
                    let user_staking_pda_filter = RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                        0,
                        &get_user_staking_anchor_discriminator(),
                    ));
                    let filters = vec![user_staking_pda_filter];
                    let existing_user_staking_accounts = program
                        .accounts::<UserStaking>(filters)
                        .await
                        .map_err(|e| backoff::Error::transient(e.into()))?;
                    let existing_user_staking_accounts_without_staking_type = {
                        let mut indexed_user_staking_accounts = indexed_user_staking_accounts.write().await;

                        // filter out the accounts that have no staking type defined yet
                        let (existing_user_staking_accounts_with_staking_type, existing_user_staking_accounts_without_staking_type): (HashMap<Pubkey, UserStaking>, HashMap<Pubkey, UserStaking>) =
                            existing_user_staking_accounts.into_iter().partition(|(_, account)| account.staking_type != 0);
                        log::info!("  <> # of existing UserStaking accounts w/o staking type defined filtered out: {}", existing_user_staking_accounts_without_staking_type.len());

                        indexed_user_staking_accounts.extend(existing_user_staking_accounts_with_staking_type);
                        // return the accounts without staking type defined
                        existing_user_staking_accounts_without_staking_type
                    };
                    log::info!(
                        "  <> # of existing UserStaking accounts parsed and loaded: {}",
                        indexed_user_staking_accounts.read().await.len()
                    );
                    existing_user_staking_accounts_without_staking_type
                };

                // Update for current Staking accounts
                update_staking_round_next_resolve_time_cache(&staking_round_next_resolve_time_cache, &indexed_staking_accounts).await;

                // Update for current UserStaking accounts
                update_claim_cache(&claim_cache, &indexed_user_staking_accounts).await;
                existing_user_staking_accounts_without_staking_type
            };
            // ////////////////////////////////////////////////////////////////

            // ////////////////////////////////////////////////////////////////
            // The account filter map is what is provided to the subscription request
            // to inform the server about the accounts we are interested in observing changes to
            // ////////////////////////////////////////////////////////////////
            log::info!("2 - Generate subscription request and open stream...");
            let accounts_filter_map =
                generate_accounts_filter_map(&indexed_user_staking_accounts).await;
            log::info!("  <> Account filter map initialized");
            let (mut subscribe_tx, mut stream) = {
                let request = SubscribeRequest {
                    ping: None,// Some(SubscribeRequestPing { id: 1 }),
                    accounts: accounts_filter_map,
                    commitment: commitment.map(|c| c.into()),
                    ..Default::default()
                };
                log::debug!("  <> Sending subscription request: {:?}", request);
                let (subscribe_tx, stream) = grpc
                    .subscribe_with_request(Some(request))
                    .await
                    .map_err(|e| backoff::Error::transient(e.into()))?;
                log::info!("  <> stream opened");
                (subscribe_tx, stream)
            };


            // ////////////////////////////////////////////////////////////////
            // Side thread to fetch the median priority fee every 5 seconds
            // ////////////////////////////////////////////////////////////////
            let median_priority_fee = Arc::new(Mutex::new(0u64));
            // Spawn a task to poll priority fees every 5 seconds
            log::info!("3 - Spawn a task to poll priority fees every 5 seconds...");
            #[allow(unused_assignments)]
            {
            periodical_priority_fees_fetching_task = Some({
                let median_priority_fee = Arc::clone(&median_priority_fee);
                tokio::spawn(async move {
                    let mut fee_refresh_interval = interval(PRIORITY_FEE_REFRESH_INTERVAL);
                    loop {
                        fee_refresh_interval.tick().await;
                        if let Ok(fee) =
                            fetch_mean_priority_fee(&client, MEAN_PRIORITY_FEE_PERCENTILE).await
                        {
                            let mut fee_lock = median_priority_fee.lock().await;
                            *fee_lock = fee;
                            log::debug!(
                                "  <> Updated median priority fee 50th percentile to : {} µLamports / cu",
                                fee
                            );
                        }
                    }
                    })
                });
            }

                    // DO a custom processing here for the accounts without staking type defined
                    // We want to call a claim stake for them in order to get the staking type defined
                    for (user_staking_account_key, user_staking_account) in existing_user_staking_accounts_without_staking_type {
                        // retrieve the owner of the UserStaking account
                        let owner_pubkey = {
                            log::info!("Claiming stake for UserStaking account: {}", user_staking_account_key);
                            let rows = db
                                .query("SELECT user_pubkey FROM ref_user_staking WHERE user_staking_pubkey = $1::TEXT", &[&user_staking_account_key.to_string()])
                                .await.map_err(|e| backoff::Error::transient(e.into()))?;
                            
                            // let row = rows.first().expect("No row for user staking account");
                            let Some(row) = rows.first() else {
                                continue;
                            };
                            Pubkey::from_str(row.get::<_, String>(0).as_str()).expect("Invalid pubkey")
                        };

                        // Run once with ADX, then with ALP
                        handlers::claim_stakes(&user_staking_account_key, &owner_pubkey, &program, *median_priority_fee.lock().await, &ADX_MINT).await?;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        handlers::claim_stakes(&user_staking_account_key, &owner_pubkey, &program, *median_priority_fee.lock().await, &ALP_MINT).await?;
                    }
                    std::process::abort(); // TESTS

            // ////////////////////////////////////////////////////////////////
            // CORE LOOP
            //
            // Here we wait for new messages from the stream and process them
            // if coming from the price update v2 accounts, we check for 
            // liquidation/sl/tp conditions on the already indexed positions if 
            // coming from the position accounts, we update the indexed positions map
            // ////////////////////////////////////////////////////////////////
            log::info!("4 - Start core loop: processing gRPC stream...");
            loop {
                // Process any stream messages
                if let Some(message) = stream.next().await {
                    match process_stream_message(
                        message.map_err(|e| backoff::Error::transient(e.into())),
                        &indexed_staking_accounts,
                        &indexed_user_staking_accounts,
                        &claim_cache,
                        &staking_round_next_resolve_time_cache,
                        &mut subscribe_tx,
                    )
                    .await
                    {
                        Ok(_) => {
                            // Stream message processed successfully - onward with the loop
                        },
                        Err(backoff::Error::Permanent(e)) => {
                            log::error!("Permanent error: {:?}", e);
                            break;
                        }
                        Err(backoff::Error::Transient { err, .. }) => {
                            log::warn!("Transient error: {:?}", err);
                            // Handle transient error without breaking the loop
                        }
                    }
                }

                // Process any resolve staking round tasks
                log::info!("5 - Process any resolve staking round tasks...");
                process_resolve_staking_rounds(&staking_round_next_resolve_time_cache, &program, *median_priority_fee.lock().await).await?;

                // Process any claim stakes tasks
                log::info!("6 - Process any claim stakes tasks...");
                process_claim_stakes(&claim_cache, &db, &indexed_user_staking_accounts, &program, *median_priority_fee.lock().await).await?;   
            }

            Ok::<(), backoff::Error<anyhow::Error>>(())
        }
        .inspect_err(|error| log::error!("failed to connect: {error}"))
    })
    .await
    .map_err(Into::into)
}


async fn process_resolve_staking_rounds(
    staking_round_next_resolve_time_cache: &StakingRoundNextResolveTimeCacheThreadSafe,
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
) -> Result<(), backoff::Error<anyhow::Error>> {
    let current_time = chrono::Utc::now().timestamp();
    let cache = staking_round_next_resolve_time_cache.read().await;

    for (staking_account_key, next_resolve_time) in cache.iter() {
        if current_time >= *next_resolve_time {
            if let Err(e) = handlers::resolve_staking_round::resolve_staking_round(
                staking_account_key,
                &program,
                median_priority_fee,
            )
            .await
            {
                log::error!("Error resolving staking round: {}", e);
            }
        }
    }
    Ok(())
}

pub async fn process_claim_stakes(
    claim_cache: &UserStakingClaimCacheThreadSafe,
    db: &tokio_postgres::Client,
    indexed_user_staking_accounts: &IndexedUserStakingAccountsThreadSafe,
    program: &Program<Arc<Keypair>>,
    median_priority_fee: u64,
) -> Result<(), backoff::Error<anyhow::Error>> {
    let current_time = chrono::Utc::now().timestamp();
    let claim_cache = claim_cache.read().await;
    for (user_staking_account_key, last_claim_time) in claim_cache.iter().filter(|(_, last_claim_time)| last_claim_time.is_some()) {
        if current_time >= last_claim_time.unwrap() + AUTO_CLAIM_THRESHOLD_SECONDS {
            // retrieve the owner of the UserStaking account
            let owner_pubkey = {
                let rows = db
                    .query("SELECT user_pubkey FROM ref_user_staking WHERE user_staking_pubkey = $1::TEXT", &[&user_staking_account_key.to_string()])
                    .await.map_err(|e| backoff::Error::transient(e.into()))?;
                
                let row = rows.first().expect("No row for user staking account");
                Pubkey::from_str(row.get::<_, String>(0).as_str()).expect("Invalid pubkey")
            };

            // Retrieve the UserStaking account
            let indexed_user_staking_accounts_read = indexed_user_staking_accounts.read().await;
            let user_staking_account = indexed_user_staking_accounts_read
                .get(user_staking_account_key)
                .expect("UserStaking account not found in the indexed user staking accounts");

            // Retrieve the staked token mint - Which might not be defined for some account as it was a late addition to the program.
            let staked_token_mint =  match user_staking_account.get_staking_type() {
                StakingType::LM => ADX_MINT,
                StakingType::LP => ALP_MINT,
            };

            // Do a claim stake for the UserStaking account if we have a staked token mint
            handlers::claim_stakes(user_staking_account_key, &owner_pubkey, &program, median_priority_fee, &staked_token_mint)
                .await
                .map_err(|e| backoff::Error::transient(anyhow::anyhow!(e)))?;
        }
    }
    Ok(())
}