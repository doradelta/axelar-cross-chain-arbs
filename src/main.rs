use axelar_arbs::*;
use ethers::{
    prelude::{Address, Ws},
    providers::{Middleware, Provider, StreamExt},
};
use num_bigint::BigInt;
use reqwest::Client;
use rust_decimal::prelude::Zero;
use std::str::FromStr;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    vec,
};
use tokio::sync::RwLock as TokioRwLock;
use axelar_arbs::SquidRouterResult;
use std::env;

// this switch allow the arb even if is not profitable
const SWITCH_ALLOW_ARB: bool = true;

// true == SQUID_ROUTER, false == PARASWAP + SQUIDROUTER (impl not finished, this would be V2)
const SWITCH_ONLY_SQUID_ROUTER: bool = true;

// this switch makes the program finish when executing 1 arb
const SWITCH_ONLY_ONE_ARB: bool = true;

// We don't allow to execute more than one arb in parallel
static ARB_PENDING: AtomicBool = AtomicBool::new(false);

static MAX_SLIPPAGE: u8 = 2; // from 0 to 99


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    // In the .json file we only have 2 tokens, for the sake of simplicity we'll create 1 pair.
    let mut chains: Vec<Chain> = serde_json::from_str(include_str!("./files/chains.json")).unwrap();

    for elem in chains.iter_mut() {
        let token_base = elem.tokens.get(0).unwrap(); // WETH
        let token_quote = elem.tokens.get(1).unwrap(); // USDC

        elem.pairs.push((token_quote.clone(), token_base.clone()));
        // we can add more pairs here
    }

    let mut handles = vec![];
    for chain in chains {
        handles.push(tokio::spawn(async move {
            // we listen blocks on each chain in parallel
            listen_blocks(chain).await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn listen_blocks(chain: Chain) {
    // add chain to CHAINS for first time
    {
        let mut chains = CHAINS.write().unwrap();
        let mut pairs = vec![];
        for pair in &chain.pairs {
            pairs.push((
                pair.clone(),
                (
                    BigInt::default(),
                    String::default(),
                    Address::default(),
                    BigInt::default(),
                ),
            ));
        }
        chains.push((chain.name.clone(), chain.id, chain.url.clone(), pairs));
        drop(chains);
    }

    let client_aggregator = Arc::new(TokioRwLock::new(Client::new()));
    let provider_ws = Provider::new(Ws::connect(chain.url.clone()).await.unwrap());
    let web3_listen = Arc::new(provider_ws);
    let mut stream = web3_listen.subscribe_blocks().await.unwrap();
    let pairs_arc = Arc::new(chain.pairs);

    while let Some(block) = stream.next().await {
        let Some(_) = block.number else { continue };
        let pairs = pairs_arc.clone();
        let mut pair_responses = Vec::with_capacity(pairs.len());

        for pair in pairs.iter() {
            let pair_clone = pair.clone();
            let amount = BigInt::from(pair_clone.1.amount_to_arb);
            // more useful to not get confused on decimals on each chain
            // let amount = BigInt::from(pair_clone.1.amount_to_arb)
            //     * BigInt::from(10).pow(pair_clone.1.decimals);

            // search best token exchange inside THIS chain (e.g. WETH -> USDC in POLYGON)
            pair_responses.push((
                pair_clone.clone(),
                amount.clone(),
                axelar_arbs::call_custom_router(
                    chain.id,
                    client_aggregator.clone(),
                    amount,
                    pair_clone,
                ),
            ));
        }

        // Loop over all the pairs inside THIS chain
        for (index, (pair, amount, pair_chain_a)) in pair_responses.into_iter().enumerate() {
            let pair_chain_a_res_aggregator = pair_chain_a.0 .2.await.unwrap();
            let pair2_chain_a_res_aggregator = pair_chain_a.1 .2.await.unwrap();

            if let Ok(pair_chain_a_res_aggregator) = pair_chain_a_res_aggregator {
                // Update the price of the pairs on this chain to be available in other chains to find arbs
                {
                    let mut chains = CHAINS.write().unwrap();
                    // This could be a HashSet to make it more efficient
                    for chain_ in chains.iter_mut() {
                        if let Ok(pair2_chain_a_res_aggregator) = &pair2_chain_a_res_aggregator {
                            if chain_.0 == chain.name {
                                if let Some(pair) = chain_.3.get_mut(index) {
                                    *pair = (pair.0.clone(), pair2_chain_a_res_aggregator.clone());
                                }
                            }
                        }
                    }
                    drop(chains);
                }

                // Find arbs for same pairs on different chains
                let chains = CHAINS.read().unwrap();
                for chain_ in chains.iter() {
                    // we're not interested on arbs on the same chain btm
                    if chain_.0 != chain.name {
                        if let Some(pair_chain_b) = chain_.3.get(index) {
                            let pair_chain_b_res_aggregator = &pair_chain_b.1;

                            let (_, amount_out_chain_b) =
                                axelar_arbs::make_same_digits(
                                    pair_chain_a_res_aggregator.0.clone(),
                                    pair_chain_b_res_aggregator.0.clone(),
                                );

                            let gas_cost_chain_a = pair_chain_a_res_aggregator.3.clone();
                            let gas_cost_chain_b = pair_chain_b_res_aggregator.3.clone();
                            let chain_a_name = chain.name.clone();
                            let chain_b_name = chain_.0.clone();
                            let chain_a_id = chain.id.clone();
                            let chain_b_id = chain_.1.clone();
                            let chain_a_url = chain.url.clone();
                            let chain_b_url = chain_.2.clone();
                            let client = client_aggregator.clone();
                            let pair = (pair.1.clone(), pair_chain_b.0 .0.clone());
                            let amount_clone = amount.clone();

                            tokio::spawn(async move {
                                try_execute_arb(
                                    amount_clone,
                                    amount_out_chain_b,
                                    gas_cost_chain_a,
                                    gas_cost_chain_b,
                                    pair,
                                    chain_a_name,
                                    chain_b_name,
                                    chain_a_id,
                                    chain_b_id,
                                    chain_a_url,
                                    chain_b_url,
                                    client,
                                )
                                .await;
                            });
                        }
                    }
                }
                drop(chains);
            }
        }
    }
}

/// Asynchronously attempts to execute an arbitrage transaction between two chains.
///
/// # Arguments
///
/// * `amount_in_chain_a` - The amount of the token to be traded in Chain A.
/// * `amount_out_chain_b` - The amount of the token expected to be received in Chain B.
/// * `gas_cost_chain_a` - The gas cost for the transaction on Chain A.
/// * `gas_cost_chain_b` - The gas cost for the transaction on Chain B.
/// * `pair` - A tuple representing the token pair involved in the arbitrage (Token A, Token B).
/// * `chain_a_name` - The name of Chain A.
/// * `chain_b_name` - The name of Chain B.
/// * `chain_a_id` - The unique identifier of Chain A.
/// * `chain_b_id` - The unique identifier of Chain B.
/// * `chain_a_url` - The URL of the RPC endpoint for Chain A.
/// * `chain_b_url` - The URL of the RPC endpoint for Chain B.
/// * `client` - A thread-safe, async-aware wrapper around an HTTP client.
/// ```
async fn try_execute_arb(
    amount_in_chain_a: BigInt,
    mut amount_out_chain_b: BigInt,
    mut gas_cost_chain_a: BigInt,
    mut gas_cost_chain_b: BigInt,
    pair: (Token, Token),
    chain_a_name: ChainName,
    chain_b_name: ChainName,
    chain_a_id: u16,
    chain_b_id: u16,
    chain_a_url: String,
    chain_b_url: String,
    client: Arc<TokioRwLock<Client>>,
) {
    if amount_in_chain_a > BigInt::zero()
        && amount_out_chain_b > BigInt::zero()
        && !ARB_PENDING.load(Ordering::Acquire)
    {
        ARB_PENDING.store(true, Ordering::Release);
        let mut res = ((BigInt::default(), SquidRouterResult::default()), (BigInt::default(), SquidRouterResult::default()));

        if SWITCH_ONLY_SQUID_ROUTER {
            res = axelar_arbs::call_squid_router(
                chain_a_id,
                chain_b_id,
                pair.0.address,
                pair.1.address,
                amount_in_chain_a.clone(),
                *MY_WALLET,
                *MY_WALLET,
                MAX_SLIPPAGE,
                client,
            )
            .await;

            if res.0.1 != SquidRouterResult::default() {
                gas_cost_chain_a = BigInt::from_str(&res.0 .1.gas_limit).unwrap();
                gas_cost_chain_b = BigInt::from_str(&res.1 .1.gas_limit).unwrap();
                amount_out_chain_b = res.1 .0;
            }
        }

        if SWITCH_ALLOW_ARB
            // this is only for V2
            || arb_is_profitable(
                &amount_out_chain_b,
                &amount_in_chain_a,
                &gas_cost_chain_b,
                &gas_cost_chain_a,
            )
        {
            let mut exit = false;
            println!(
                "Expected gas cost in {:?}: {}",
                chain_a_name, gas_cost_chain_a
            );
            println!(
                "Expected gas cost in {:?}: {}",
                chain_b_name, gas_cost_chain_b
            );
            println!(
                "Expected profit: {} {}",
                &amount_out_chain_b - &amount_in_chain_a,
                pair.0.name
            );

            if SWITCH_ONLY_SQUID_ROUTER && res.0.1 != SquidRouterResult::default() && (SWITCH_ALLOW_ARB || amount_out_chain_b > amount_in_chain_a) {
                let res0 = tokio::spawn(async move {
                    send_tx(
                        res.0.1.target_address,
                        res.0.1.data,
                        chain_a_id as u64,
                        res.0.1.gas_limit.parse::<u64>().unwrap(),
                        res.0.1.max_fee_per_gas.parse::<u64>().unwrap(),
                        res.0.1.max_priority_fee.parse::<u64>().unwrap(),
                        res.0.1.value.parse::<u128>().unwrap(),
                        chain_a_url,
                    )
                    .await;
                });

                let res1 = tokio::spawn(async move {
                    send_tx(
                        res.1.1.target_address,
                        res.1.1.data,
                        chain_b_id as u64,
                        res.1.1.gas_limit.parse::<u64>().unwrap(),
                        res.1.1.max_fee_per_gas.parse::<u64>().unwrap(),
                        res.1.1.max_priority_fee.parse::<u64>().unwrap(),
                        res.1.1.value.parse::<u128>().unwrap(),
                        chain_b_url,
                    )
                    .await;
                });

                // TODO: handle results and check profit
                let _res0 = res0.await;
                let _res1 = res1.await;
                exit = true;
            }

            if SWITCH_ONLY_ONE_ARB && exit {
                std::process::exit(0);
            }
        }

        ARB_PENDING.store(false, Ordering::Release);
    }
}


/// Sends a transaction to the specified address on the blockchain.
///
/// # Arguments
///
/// * `to` - The address to which the transaction is sent.
/// * `data` - The transaction data to be sent.
/// * `chain_id` - The identifier of the blockchain network.
/// * `gas` - The gas limit for the transaction.
/// * `max_fee_per_gas` - The maximum fee per gas for the transaction.
/// * `max_priority_fee` - The maximum priority fee for the transaction.
/// * `value` - The amount of value to send with the transaction.
/// * `url` - The URL of the RPC endpoint for the blockchain network.
/// ```
pub async fn send_tx(
    to: Address,
    data: String,
    chain_id: u64,
    gas: u64,
    max_fee_per_gas: u64,
    max_priority_fee: u64,
    value: u128,
    mut url: String,
) {

    let prvk = secp256k1::SecretKey::from_str(
        &env::var("MY_PRIVATE_KEY").unwrap_or_else(|_| "".to_string()),
    )
    .unwrap();

    // EIP-1559
    let tx_object = web3::types::TransactionParameters {
        to: Some(to),
        gas: web3::types::U256::from(gas),
        value: web3::types::U256::from(value),
        data: web3::types::Bytes::from(hex::decode(data[2..].to_string()).unwrap()),
        chain_id: Some(chain_id),
        transaction_type: Some(web3::types::U64::from(2)),
        access_list: None,
        max_fee_per_gas: Some(web3::types::U256::from(max_fee_per_gas * 2)),
        max_priority_fee_per_gas: Some(web3::types::U256::from(max_priority_fee * 2)),
        ..Default::default()
    };

    url = url.replace("wss://", "https://");
    let web3_query = web3::Web3::new(web3::transports::Http::new(&url).unwrap());
    let signed = web3_query
        .accounts()
        .sign_transaction(tx_object, &prvk)
        .await
        .unwrap();

    let _tx_hash = web3_query
        .eth()
        .send_raw_transaction(signed.raw_transaction)
        .await
        .unwrap();

    println!("0x{:x}", _tx_hash);
}


/// Checks if an arbitrage opportunity is profitable.
///
/// # Arguments
///
/// * `amount_in_chain_a` - The amount of the token to be traded in Chain A.
/// * `amount_out_chain_b` - The amount of the token expected to be received in Chain B.
/// * `_gas_cost_chain_a` - The gas cost for the transaction on Chain A (unused in the current implementation).
/// * `_gas_cost_chain_b` - The gas cost for the transaction on Chain B (unused in the current implementation).
///
/// # Returns
///
/// * `true` if the arbitrage opportunity is profitable, otherwise `false`.
/// ```
fn arb_is_profitable(
    amount_in_chain_a: &BigInt,
    amount_out_chain_b: &BigInt,
    _gas_cost_chain_a: &BigInt,
    _gas_cost_chain_b: &BigInt,
) -> bool {
    // if token_out chainB > token_in chainA
    if amount_out_chain_b > amount_in_chain_a {
        if SWITCH_ONLY_SQUID_ROUTER {
            // TODO:
            // convert gas of each chain into $ and convert amounts of each chain into $
            // amount_out_chain_b - amount_in_chain_a - gas_cost_chain_a - gas_cost_chain_b > MIN_PROFIT
            false
        } else {
            // TODO:
            // get the gas of the Multicall (swap & Squid) on chainA & the Multicall (swap & Squid) on chainB
            false
        }
    } else {
        false
    }
}
