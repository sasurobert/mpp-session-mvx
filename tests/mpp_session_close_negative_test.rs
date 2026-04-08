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

#[test]
fn test_close_invalid_signature() {
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
                Account::new().balance("10000000000000000000").nonce(1),
            )
            .put_account(
                &receiver_address,
                Account::new().balance("100000000000000000").nonce(1),
            )
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("5000000000000000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    let invalid_signature = [0u8; 64];
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&invalid_signature[..])
            .expect(TxExpect::err(10, "str:ed25519 verify error")),
    );
}

#[test]
fn test_close_already_closed() {
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
                Account::new().balance("10000000000000000000").nonce(1),
            )
            .put_account(
                &receiver_address,
                Account::new().balance("100000000000000000").nonce(1),
            )
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("5000000000000000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Close with valid voucher
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 1_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Try to close again
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 2_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("2000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig2[..])
            .expect(TxExpect::user_error("str:Session already closed")),
    );
}

#[test]
fn test_close_stale_nonce() {
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
                Account::new().balance("10000000000000000000").nonce(1),
            )
            .put_account(
                &receiver_address,
                Account::new().balance("100000000000000000").nonce(1),
            )
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("5000000000000000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Settle with nonce=2
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 1_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Try to close with nonce=1 (stale, since session.nonce is now 2, and close requires nonce >= session.nonce)
    // Actually close requires nonce >= session.nonce so nonce=1 < 2 would fail
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 2_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("2000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig2[..])
            .expect(TxExpect::user_error("str:Stale voucher nonce")),
    );
}

#[test]
fn test_close_zero_refund() {
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
                Account::new().balance("10000000000000000000").nonce(1),
            )
            .put_account(
                &receiver_address,
                Account::new().balance("100000000000000000").nonce(1),
            )
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    let deposit = 5_000_000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("5000000") // small deposit to make math easier
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let channel_id = compute_channel_id(&employer_address, &receiver_address);

    // Close with amount == locked (no refund to employer, all goes to receiver)
    let sig = sign_voucher(&signing_key, &sc_address, &channel_id, deposit, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("5000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig[..]),
    );

    // Receiver gets all 5_000_000 + initial 100000000000000000, SC has 0
    world.check_state_step(
        CheckStateStep::new()
            .put_account(
                &receiver_address,
                CheckAccount::new().balance("100000000005000000"),
            )
            .put_account(&sc_address, CheckAccount::new().balance("0")),
    );
}

#[test]
fn test_close_zero_release() {
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
                Account::new().balance("10000000000000000000").nonce(1),
            )
            .put_account(
                &receiver_address,
                Account::new().balance("100000000000000000").nonce(1),
            )
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

    // Settle 3_000_000 first
    let sig1 = sign_voucher(&signing_key, &sc_address, &channel_id, 3_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("3000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig1[..]),
    );

    // Close with amount == already_settled (3_000_000), so zero new release, 2_000_000 refund
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 3_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("3000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig2[..]),
    );

    // Receiver only got 3_000_000 total + initial 100000000000000000, employer gets refund of 2_000_000
    world.check_state_step(
        CheckStateStep::new()
            .put_account(
                &receiver_address,
                CheckAccount::new().balance("100000000003000000"),
            )
            .put_account(&sc_address, CheckAccount::new().balance("0")),
    );
}
