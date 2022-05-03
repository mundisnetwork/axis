<div align="center">
<p>
  <img src="docs/img/mundis-logo.svg" 
    width="150" 
    alt="A layered metaverse ecosystem of parallel, interconnected worlds, built for massive scale, extreme performance, visual interaction and unlimited extensibility."
  />
</p>
<h1>MUNDIS</h1>
<h3>Layer 0 Blockchain Node</h3>
</div>

# TL;DR
The role of L0 is to coordinate the Mundis metaverse. It's a core blockchain based on the Solana codebase that features a global timeline of events with an optimized pBFT replicated state machine that can do sub-second finality times. It runs on a globally distributed "clock" with Proof-of-Stake (PoS) consensus, built to address the needs of our Metaverse.

The global "clock" is not a consensus algorithm but a computational algorithm that provides a way to cryptographically verify the ordering of events to solve the agreement on global time. It's a VDF hash-chain used to checkpoint L1 chains and coordinate global consensus. It allows near-instant finality up to hundreds of thousands of transactions per second.

The L0 chain needs actors that can process transactions and participate in consensus. These actors are named validators in existing blockchains, but in Mundis, we call them **Architects**.

Architects are the keepers of the Metaverse. Besides validation tasks for transactions and consensus, they also participate in global governance. Therefore, it is desirable to have many Architects for the ecosystem to be decentralized, and Mundis does not limit this number as some existing blockchains do. As the Metaverse grows, more Architects will be needed to increase the performance of the ecosystem.
