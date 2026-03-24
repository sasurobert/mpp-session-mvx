# MultiversX Machine Payments Protocol (MPP) - Session Facilitator

This repository contains the MultiversX smart contract implementation for the Machine Payments Protocol (MPP) state channel sessions. It is part of the agentic-commerce ecosystem, enabling high-frequency, non-custodial agent-to-agent micro-payments.

## Architecture

The `mpp-session-mvx` contract acts as a state channel facilitator:
1. **Open Session**: A user or agent locks tokens (e.g., ESDT) in escrow.
2. **Stream Vouchers**: Agents continuously stream off-chain `Ed25519` signed vouchers authorizing incremental payments.
3. **Settle Session**: The receiver submits the latest cumulative voucher to the contract to claim exactly what they are owed.
4. **Close Session**: Either party can safely close the session; the receiver claims the agreed amount, and the employer receives the unspent remainder.

## Key Features

- **Semantic Domain Separation**: Protects against cross-network replay attacks using `mpp-session-v1` prefixing.
- **Deterministic Channel IDs**: Built using `keccak256(employer, receiver, token_identifier, nonce)`.
- **Cumulative Voucher Pattern**: Eliminates the need to track individual transactions on-chain.

## Core Endpoints
- `open`: Initialize a session by depositing an initial escrow.
- `top_up`: Add additional funds to an existing session.
- `settle`: Claim funds using a cryptographically signed voucher.
- `request_close`: Propose closing the session if the session hasn't been engaged.
- `close`: Finalize the session, distributing the payment to the receiver and refunding the remaining escrow to the employer.

## Testing & Automation
This contract is covered by end-to-end `mx-chain-simulator-go` and `rust-vm` testing pipelines, ensuring deterministic transaction fulfillment and gas efficiency within the MultiversX WASM ecosystem.

## License
MIT License
