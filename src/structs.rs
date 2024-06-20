use ethers_core::types::Address;
use lazy_static::lazy_static;
use num_bigint::BigInt;
use serde::Deserialize;
use std::{
    str::FromStr,
    sync::{Arc, RwLock},
};

// Static initialization of MY_WALLET and CHAINS
lazy_static! {
    /// The address of the wallet.
    pub static ref MY_WALLET: Address = {
        // Access the environment variable and parse it into an Address
        let address_str = std::env::var("MY_WALLET_ADDRESS")
            .expect("MY_WALLET_ADDRESS environment variable is not defined");
        Address::from_str(&address_str)
            .expect("Failed to parse Address from environment variable")
    };

    /// The collection of chains and their associated pairs.
    #[derive(Debug)]
    pub static ref CHAINS: Arc<RwLock<Vec<(
        ChainName,
        u16,    // ChaindId
        String, // url
        Vec<(
            (Token, Token),
            (BigInt, String, Address, BigInt)
        )>
    )>>> = Arc::new(RwLock::new(Vec::new()));
}

/// Represents the result of a Squid Router transaction.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct SquidRouterResult {
    /// The target address for the transaction.
    pub target_address: Address,
    /// The transaction data.
    pub data: String,
    /// The value to be sent with the transaction.
    pub value: String,
    /// The gas limit for the transaction.
    pub gas_limit: String,
    /// The maximum fee per gas for the transaction.
    pub max_fee_per_gas: String,
    /// The maximum priority fee for the transaction.
    pub max_priority_fee: String,
}

/// Represents the name of a blockchain.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum ChainName {
    /// Binance Smart Chain.
    BSC,
    /// Polygon blockchain.
    Polygon,
}

/// Represents a token.
#[derive(Debug, Deserialize, Clone)]
pub struct Token {
    /// The name of the token.
    pub name: String,
    /// The address of the token.
    pub address: Address,
    /// The number of decimals for the token.
    pub decimals: u32,
    /// The amount of the token used for arbitrage.
    pub amount_to_arb: u128,
}

/// Represents a blockchain.
#[derive(Debug, Deserialize)]
pub struct Chain {
    /// The unique identifier of the blockchain.
    pub id: u16,
    /// The name of the blockchain.
    pub name: ChainName,
    /// The URL of the blockchain.
    pub url: String,
    /// The list of tokens on the blockchain.
    pub tokens: Vec<Token>,
    /// The list of pairs on the blockchain.
    pub pairs: Vec<(Token, Token)>,
}
