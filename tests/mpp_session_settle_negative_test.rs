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
fn test_settle_closed_session() {
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

    // Close the session via request_close after deadline
    world.set_state_step(SetStateStep::new().block_timestamp_seconds(10001));
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..]),
    );

    // Try to settle on closed session
    let sig = sign_voucher(&signing_key, &sc_address, &channel_id, 1_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig[..])
            .expect(TxExpect::user_error("str:Session already closed")),
    );
}

#[test]
fn test_settle_stale_nonce() {
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

    // First settle with nonce=1
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

    // Try to settle again with nonce=1 (stale)
    let sig_stale = sign_voucher(&signing_key, &sc_address, &channel_id, 2_000_000, 1);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("2000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&sig_stale[..])
            .expect(TxExpect::user_error("str:Stale voucher nonce")),
    );
}

#[test]
fn test_settle_invalid_amount() {
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

    // First settle with 2_000_000
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

    // Try to settle with amount=1_000_000 which is less than already settled (2_000_000)
    let sig2 = sign_voucher(&signing_key, &sc_address, &channel_id, 1_000_000, 2);
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&2u64.to_be_bytes()[..])
            .argument(&sig2[..])
            .expect(TxExpect::user_error("str:Invalid settlement amount")),
    );
}
