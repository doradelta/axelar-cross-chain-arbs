

Structure:
.
├── cross_chain_arb_V2.png  // V2 implementation
├── example                 // a cross-chain arb example using SquidRouter
└── src
     ├── files
     │   └── chains.json    // Chains info
     ├── lib.rs
     ├── main.rs            // core-program to find arbs on N chains on M pairs
     ├── routers.rs         // simulate routes in Custom router (Paraswap) & SquidRouter
     ├── structs.rs
     └── utils.rs
     

How to run the program:
- fill the .env (assuming your EVM WALLET is the same in all chains):
  MY_WALLET_ADDRESS=""
  MY_PRIVATE_KEY=""
  
- cargo run --release    

     
What the program does:

The program begins by reading information from the chains.json file, the program is made so that the desired number of chains and pairs can be inserted,
however, it will not work with more than 3 chains and 3 pairs due to the rate limit in Paraswap and SquidRouter (I explain more about routers later).

The program listen for new blocks from each chain in parallel, and every time a new block is produced, it does two simulations in Paraswap, 
swap base->quote(fixed) & swap quote(fixed)->base on the same chain (as is fixed in size, this simulation can be done in parallel).
If it were pool monitoring instead of Paraswap we would look at LogsBloom to see if there has been any change in that pool.

In addition, it also simulates in SquidRouter cross-chain swap base(chainA)->quote(chainB) & quote(chainB)->base(chainA), this simulation cannot be done in parallel.

Compare arb in Paraswap and compare arb in SquidRouter.
V1 (finished): if profitable SquidRouter, run arb
V2 (not finished): if profitable Paraswap, 2 options: see if it can join the SquidRouter execution OR run arb (no bridges and manual rebalance)


Clarifications:

In short, V1 focuses on the SquidRouter, the project contains a file called "example" where we can see a cross-chain arb executed by the bot (not profitable)
and an image called "cross_chain_arb_V2.png" where it explains how it works of the integration with Paraswap, although Paraswap is not necessary 
and can be replaced with pool monitoring.
In main.rs there is a series of FLAGS that make the program run under certain conditions, I recommend taking a look at it.
Also, in routers.rs there is a DEBUG flag to see more information when an arbitrage is simulated in Squidrouter.


Routers:

I have never liked using external routers, that's why I built my own. I have used Paraswap just to give an example.
As for the SquidRouter, things get even worse. Making a query takes ages (about 8-10 seconds each query).
At first I deployed 2 SmartContracts in each chain, to use Axelar at a low level without having to use SquidRouter,
but I ran into the problem that GMP express is only available in SquidRouter or in Axelar Partners, without GMP express,
A transaction can take 1 week in the worst case and 20 minutes on average.
Btw, GMP express is a good idea although a bit shabby in my opinion (open to discuss)


** WARNING ** 
What is missing:
- You need to approve you wallet for the tokens on the chains you want to swap (you can do this in the SquidRouter UI)
- Arb does not take into account gas fees.
- Tx receipt and check real profit is missing
- I have tested the program in many ways and informally, I have not written the cover tests.
- Disclaimer: This program is far from being perfect, I have done what I could with the time I have managed to get, I don't have more time this week



