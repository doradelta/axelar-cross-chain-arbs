use crate::SquidRouterResult;

use super::structs::Token;
use ethers::prelude::Address;
use num_bigint::BigInt;
use reqwest::Client;
use rust_decimal::prelude::Zero;
use serde_json::Value;
use std::{str::FromStr, sync::Arc};
use tokio::sync::RwLock as TokioRwLock;
use tokio::task::JoinHandle;

const DEBUG: bool = true;

/// Calls the Squid Router contract 2 times to simulate an arbitrage between two chains.
///
/// # Arguments
///
/// * `chain_a_id` - The unique identifier of Chain A.
/// * `chain_b_id` - The unique identifier of Chain B.
/// * `token_in_chain_a` - The address of the token to be swapped in Chain A.
/// * `token_out_chain_b` - The address of the token to be received in Chain B.
/// * `amount` - The amount of the token to be swapped.
/// * `from_address_chain_a` - The sender's address in Chain A.
/// * `to_address_chain_b` - The recipient's address in Chain B.
/// * `slippage` - The acceptable slippage percentage (from 0 to 99).
/// * `client_aggregator` - The HTTP client aggregator.
///
/// # Returns
///
/// A tuple containing two elements:
/// - The result of the transaction in Chain A.
/// - The result of the transaction in Chain B.
///
/// Each result contains the amount swapped and additional transaction information.
/// ```
pub async fn call_squid_router(
    chain_a_id: u16,
    chain_b_id: u16,
    token_in_chain_a: Address,
    token_out_chain_b: Address,
    amount: BigInt,
    from_address_chain_a: Address,
    to_address_chain_b: Address,
    slippage: u8, // from 0 to 99
    client_aggregator: Arc<TokioRwLock<Client>>,
) -> ((BigInt, SquidRouterResult), (BigInt, SquidRouterResult)) {
    let res0 = simulate_squid_router(
        chain_a_id,
        chain_b_id,
        token_in_chain_a,
        token_out_chain_b,
        &amount,
        from_address_chain_a,
        to_address_chain_b,
        slippage,
        client_aggregator.clone(),
    )
    .await;

    let res1 = simulate_squid_router(
        chain_b_id,
        chain_a_id,
        token_out_chain_b,
        token_in_chain_a,
        &res0.clone().unwrap().0,
        to_address_chain_b,
        from_address_chain_a,
        slippage,
        client_aggregator,
    )
    .await;

    (res0.unwrap(), res1.unwrap())
}


/// Simulates a transaction on the Squid Router contract between two chains.
///
/// This function does not execute the transaction but returns a simulation result.
///
/// # Arguments
///
/// * `chain_a_id` - The unique identifier of Chain A.
/// * `chain_b_id` - The unique identifier of Chain B.
/// * `token_in_chain_a` - The address of the token to be swapped in Chain A.
/// * `token_out_chain_b` - The address of the token to be received in Chain B.
/// * `amount` - The amount of the token to be swapped.
/// * `from_address_chain_a` - The sender's address in Chain A.
/// * `to_address_chain_b` - The recipient's address in Chain B.
/// * `slippage` - The acceptable slippage percentage (from 0 to 99).
/// * `client_aggregator` - The HTTP client aggregator.
///
/// # Returns
///
/// A result containing either:
/// - The amount swapped and additional transaction information if the simulation is successful.
/// - An error message if the simulation fails.
/// ```
pub async fn simulate_squid_router(
    chain_a_id: u16,
    chain_b_id: u16,
    token_in_chain_a: Address,
    token_out_chain_b: Address,
    amount: &BigInt,
    from_address_chain_a: Address,
    to_address_chain_b: Address,
    slippage: u8, // from 0 to 99
    client_aggregator: Arc<TokioRwLock<Client>>,
) -> Result<(BigInt, SquidRouterResult), String> {
    let mut amount_out = BigInt::zero();
    let mut result = SquidRouterResult::default();

    let url = format!("https://api.0xsquid.com/v1/route?fromChain={chain_a_id}&toChain={chain_b_id}&fromToken=0x{:x}&toToken=0x{:x}&fromAmount={amount}&fromAddress={:x}&toAddress=0x{:x}&slippage={slippage}&quoteOnly=false&enableExpress=true&receiveGasOnDestination=false",
        token_in_chain_a, token_out_chain_b, from_address_chain_a, to_address_chain_b
        );

    if DEBUG {
        println!("{url}");
    }

    let client_aggregator_read = client_aggregator.read().await;
    let res = client_aggregator_read
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    drop(client_aggregator_read);

    let body = res.text().await.map_err(|err| err.to_string())?;
    let json_value = serde_json::from_str::<Value>(&body).map_err(|err| err.to_string())?;
    

    if DEBUG {
        println!("{:#?}", json_value);
    }
    
    if let Some(res) = json_value.get("route") {
        if let Some(res) = res.get("estimate") {
            if let Some(res) = res.get("toAmountMin") {
                amount_out = BigInt::from_str(&format!("{}", res.to_string().trim_matches('"')))
                    .map_err(|err| err.to_string())?;
            }
        }
        if let Some(res) = res.get("transactionRequest") {
            if let Some(res) = res.get("targetAddress") {
                result.target_address = Address::from_str(res.to_string().trim_matches('"'))
                    .map_err(|err| err.to_string())?;
            }
            if let Some(res) = res.get("data") {
                result.data = res.to_string().trim_matches('"').to_string();
            }
            if let Some(res) = res.get("value") {
                result.value = res.to_string().trim_matches('"').to_string();
            }
            if let Some(res) = res.get("gasLimit") {
                result.gas_limit = res.to_string().trim_matches('"').to_string();
            }
            if let Some(res) = res.get("maxFeePerGas") {
                result.max_fee_per_gas = res.to_string().trim_matches('"').to_string();
            }
            if let Some(res) = res.get("maxPriorityFeePerGas") {
                result.max_priority_fee = res.to_string().trim_matches('"').to_string();
            }
        }
    }

    Ok((amount_out, result))
}


/// Calls a custom router contract to simulate an arbitrage between two tokens on the same chain.
///
/// # Arguments
///
/// * `chain_id` - The unique identifier of the blockchain.
/// * `client_aggregator` - The HTTP client aggregator.
/// * `amount` - The amount of the token to be swapped.
/// * `pair_clone` - The pair of tokens involved in the arbitrage.
///
/// # Returns
///
/// A tuple containing two elements, each representing the call to the custom router contract:
/// - The first element includes the target address, data, and a join handle for the asynchronous call.
/// - The second element includes the target address, data, and a join handle for the asynchronous call.
///
/// Each join handle returns either:
/// - The amount swapped, transaction data, target address, and gas limit if the call is successful.
/// - An error message if the call fails.
/// ```
pub fn call_custom_router(
    chain_id: u16,
    client_aggregator: Arc<TokioRwLock<Client>>,
    amount: BigInt,
    pair_clone: (Token, Token),
) -> (
    (
        String,
        String,
        JoinHandle<Result<(BigInt, String, Address, BigInt), String>>,
    ),
    (
        String,
        String,
        JoinHandle<Result<(BigInt, String, Address, BigInt), String>>,
    ),
) {
    // token_base -> token_quote (variable amount_in -> fixed amount_out)
    let client = client_aggregator.clone();
    let amount_clone = amount.clone();

    let res0 = (
        pair_clone.0.name.clone(),
        pair_clone.1.name.clone(),
        tokio::spawn(async move {
            simulate_swap_paraswap(
                "BUY".to_string(),
                chain_id,
                &amount_clone,
                pair_clone.0.address,
                pair_clone.1.address,
                pair_clone.0.decimals,
                pair_clone.1.decimals,
                *super::structs::MY_WALLET,
                *super::structs::MY_WALLET, // *MY_SC
                client,
            )
            .await
        }),
    );

    // token_quote -> token_base (fixed amount_in -> variable amount_out)
    let client = client_aggregator.clone();
    let res1 = (
        pair_clone.1.name.clone(),
        pair_clone.0.name.clone(),
        tokio::spawn(async move {
            simulate_swap_paraswap(
                "SELL".to_string(),
                chain_id,
                &amount,
                pair_clone.1.address,
                pair_clone.0.address,
                pair_clone.1.decimals,
                pair_clone.0.decimals,
                *super::structs::MY_WALLET,
                *super::structs::MY_WALLET, // *MY_SC
                client,
            )
            .await
        }),
    );

    (res0, res1)
}

/// Simulates a swap transaction using Paraswap.
///
/// This function does not execute the transaction but returns a simulation result.
///
/// # Arguments
///
/// * `side` - The side of the swap transaction (e.g., "buy" or "sell").
/// * `chain_id` - The unique identifier of the blockchain.
/// * `amount_in` - The amount of the token to be swapped.
/// * `token_in` - The address of the token to be swapped.
/// * `token_out` - The address of the token to be received.
/// * `token0_decimals` - The number of decimals for the token to be swapped.
/// * `token1_decimals` - The number of decimals for the token to be received.
/// * `wallet_address` - The address of the wallet initiating the transaction.
/// * `receiver_address` - The address of the receiver.
/// * `client_aggregator` - The HTTP client aggregator.
///
/// # Returns
///
/// A result containing either:
/// - The amount swapped, transaction data, target address, and gas limit if the simulation is successful.
/// - An error message if the simulation fails.
/// ```
/// // approve tokens in 0x216b4b4ba9f3e719726886d34a177484278bfcae for Polygon & BSC
pub async fn simulate_swap_paraswap(
    side: String,
    chain_id: u16,
    amount_in: &BigInt,
    token_in: Address,
    token_out: Address,
    token0_decimals: u32,
    token1_decimals: u32,
    wallet_address: Address,
    receiver_address: Address,
    client_aggregator: Arc<TokioRwLock<Client>>,
) -> Result<(BigInt, String, Address, BigInt), String> {
    let mut res_amount = BigInt::from(0);
    let mut res_data = String::from("None");
    let mut res_to = Address::zero();

    let url = format!("https://apiv5.paraswap.io/prices?srcToken=0x{:x}&srcDecimals={}&destToken=0x{:x}&destDecimals={}&amount={}&side={side}&network={chain_id}&maxImpact=10",
        token_in, token0_decimals, token_out, token1_decimals, amount_in);

    let client_aggregator_read = client_aggregator.read().await;
    let res = client_aggregator_read
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    drop(client_aggregator_read);

    let body = res.text().await.map_err(|err| err.to_string())?;
    let json_value = serde_json::from_str::<Value>(&body).map_err(|err| err.to_string())?;

    match json_value.get("priceRoute") {
        Some(json) => {
            let (amount_in, amount_out, mode) = if side == "SELL".to_string() {
                (amount_in.clone(), BigInt::from(1), "destAmount")
            } else {
                (BigInt::from(1), amount_in.clone(), "srcAmount")
            };

            let dest_amount = json.get(mode).ok_or("Failed to get destination amount")?;

            res_amount =
                BigInt::from_str(format!("{}", dest_amount.to_string().trim_matches('"')).as_str())
                    .map_err(|err| err.to_string())?;

            let url = format!(
                "https://apiv5.paraswap.io/transactions/{chain_id}?gasPrice=50000000000&ignoreChecks=true&ignoreGasEstimate=true&onlyParams=false"
            );

            let body_0 = serde_json::json!({
                "srcToken": format!("0x{:x}", token_in),
                "destToken": format!("0x{:x}", token_out),
                "srcAmount": format!("{}", amount_in),
                "destAmount": format!("{}", amount_out),
                "priceRoute": json,
                "userAddress": format!("0x{:x}", wallet_address),
                "txOrigin": format!("0x{:x}", receiver_address),
                //"receiver": format!("0x{:x}", *MY_SC),
                "partner": "paraswap.io",
                "srcDecimals": token0_decimals,
                "destDecimals": token1_decimals
            });

            let client_aggregator_read = client_aggregator.read().await;
            let res = client_aggregator_read
                .post(url)
                .json(&body_0)
                .send()
                .await
                .map_err(|err| err.to_string())?;
            drop(client_aggregator_read);

            let body = res.text().await.map_err(|err| err.to_string())?;

            let json_value =
                serde_json::from_str::<serde_json::Value>(&body).map_err(|err| err.to_string())?;

            match json_value.get("to") {
                Some(address) => {
                    res_to = Address::from_str(address.as_str().ok_or("Failed to get address")?)
                        .map_err(|err| err.to_string())?;
                    let data = json_value.get("data").ok_or("Failed to get data")?;
                    res_data = format!("{}", data.to_string().trim_matches('"'));
                }
                None => {
                    println!(
                        "Failed getting calldata in Paraswap (weird): {:#}",
                        json_value
                    );
                }
            }
        }
        None => {
            println!(
                "Failed getting price in Paraswap (maybe token doesn't exist): {:#}",
                body
            );
        }
    }

    Ok((res_amount, res_data, res_to, BigInt::zero()))
}
