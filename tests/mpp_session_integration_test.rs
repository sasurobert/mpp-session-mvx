use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use multiversx_sc::types::Address;
use multiversx_sc_scenario::imports::*;

fn world() -> ScenarioWorld {
    let mut blockchain = ScenarioWorld::new();
    blockchain.register_contract(
        "file:output/mpp-session-mvx.wasm",
        mpp_session_mvx::ContractBuilder,
    );
    blockchain
}

fn sign_voucher(
    signing_key: &SigningKey,
    sc_address: &Address,
    channel_id: &[u8],
    amount: u64,
    nonce: u64,
) -> [u8; 64] {
    let mut message = Vec::new();
    message.extend_from_slice(b"mpp-session-v1");
    message.extend_from_slice(sc_address.as_bytes());
    message.extend_from_slice(channel_id);

    let mut amount_vec = amount.to_be_bytes().to_vec();
    while amount_vec.len() > 1 && amount_vec[0] == 0 {
        amount_vec.remove(0);
    }
    message.extend_from_slice(&amount_vec);
    message.extend_from_slice(&nonce.to_be_bytes());

    let hash = multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&message);
    signing_key.sign(hash.as_slice()).to_bytes()
}

fn compute_channel_id(employer: &Address, receiver: &Address) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(employer.as_bytes());
    msg.extend_from_slice(receiver.as_bytes());
    msg.extend_from_slice(&1u64.to_be_bytes());
    multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&msg).to_vec()
}

/// Full lifecycle: open → top_up → settle → close
#[test]
fn test_full_lifecycle_with_topup() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    let employer_priv_key: [u8; 32] = [1u8; 32];
    let signing_key = SigningKey::from_bytes(&employer_priv_key);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let employer_address = Address::from(verifying_key.to_bytes());
    let receiver_address = Address::from([2u8; 32]);
    let sc_address = Address::from([
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 3,
    ]);

    world.set_state_step(
        SetStateStep::new()
            .put_account(
                &employer_address,
                Account::new().balance("10000000").nonce(1),
            )
            .put_account(&receiver_address, Account::new().balance("100000").nonce(1))
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    // Step 1: Open with 3_000_000
    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("3000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Step 2: Top up with 2_000_000 more (total locked = 5_000_000)
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("2000000")
            .function("top_up")
            .argument(&channel_id[..]),
    );

    // SC should hold 5_000_000
    world.check_state_step(
        CheckStateStep::new().put_account(&sc_address, CheckAccount::new().balance("5000000")),
    );

    // Step 3: Settle 2_000_000
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 2_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("2000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Verify: receiver got 2_000_000, SC has 3_000_000 left
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("2100000"))
            .put_account(&sc_address, CheckAccount::new().balance("3000000")),
    );

    // Step 4: Close with total 4_000_000 authorized
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 4_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("4000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig2[..]),
    );

    // Final: receiver gets 2_000_000 more (total 4M), employer refunded 1_000_000
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("4100000"))
            .put_account(&sc_address, CheckAccount::new().balance("0")),
    );
}

/// Streaming payments: open → settle(n=1) → settle(n=2) → settle(n=3) → close
#[test]
fn test_streaming_payments() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    let employer_priv_key: [u8; 32] = [1u8; 32];
    let signing_key = SigningKey::from_bytes(&employer_priv_key);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let employer_address = Address::from(verifying_key.to_bytes());
    let receiver_address = Address::from([2u8; 32]);
    let sc_address = Address::from([
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 3,
    ]);

    world.set_state_step(
        SetStateStep::new()
            .put_account(
                &employer_address,
                Account::new().balance("10000000").nonce(1),
            )
            .put_account(&receiver_address, Account::new().balance("100000").nonce(1))
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("9000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Settle 1: cumulative 1_000_000
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 1_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Settle 2: cumulative 3_000_000 (delta = 2_000_000)
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 3_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("3000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig2[..]),
    );

    // Settle 3: cumulative 6_000_000 (delta = 3_000_000)
    let sig3 = sign_voucher(&signing_key, &sc_address, &channel_id, 6_000_000, 3);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("6000000")
            .argument(&3u64.to_be_bytes()[..])
            .argument(&sig3[..]),
    );

    // Verify: receiver has 100000 + 6_000_000 = 6_100_000
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("6100000"))
            .put_account(&sc_address, CheckAccount::new().balance("3000000")),
    );

    // Close with cumulative 7_000_000 (delta = 1_000_000, refund = 2_000_000)
    let sig_close = sign_voucher(&signing_key, &sc_address, &channel_id, 7_000_000, 4);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("7000000")
            .argument(&4u64.to_be_bytes()[..])
            .argument(&sig_close[..]),
    );

    // Final: receiver = 100000 + 7_000_000 = 7_100_000
    // employer = 10_000_000 - 9_000_000 + 2_000_000 = 3_000_000
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("7100000"))
            .put_account(&employer_address, CheckAccount::new().balance("3000000"))
            .put_account(&sc_address, CheckAccount::new().balance("0")),
    );
}

/// Partial settle then request_close after deadline
#[test]
fn test_partial_settle_then_request_close() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    let employer_priv_key: [u8; 32] = [1u8; 32];
    let signing_key = SigningKey::from_bytes(&employer_priv_key);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let employer_address = Address::from(verifying_key.to_bytes());
    let receiver_address = Address::from([2u8; 32]);
    let sc_address = Address::from([
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 3,
    ]);

    world.set_state_step(
        SetStateStep::new()
            .put_account(
                &employer_address,
                Account::new().balance("10000000").nonce(1),
            )
            .put_account(&receiver_address, Account::new().balance("100000").nonce(1))
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("5000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Settle only 2_000_000 out of 5_000_000
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 2_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("2000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Fast forward past deadline
    world.set_state_step(SetStateStep::new().block_timestamp_seconds(10001));

    // Employer request_close — should refund 3_000_000 (5M - 2M settled)
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..]),
    );

    // Final balances:
    // receiver = 100000 + 2_000_000 = 2_100_000
    // employer = 10_000_000 - 5_000_000 + 3_000_000 = 8_000_000
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("2100000"))
            .put_account(&employer_address, CheckAccount::new().balance("8000000"))
            .put_account(&sc_address, CheckAccount::new().balance("0")),
    );
}
