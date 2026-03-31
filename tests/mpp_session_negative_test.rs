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

#[test]
fn test_negative_flow_invalid_signature() {
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
            .egld_value("5000000000000000000") // 5 EGLD
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let mut channel_id_msg = Vec::new();
    channel_id_msg.extend_from_slice(employer_address.as_bytes());
    channel_id_msg.extend_from_slice(receiver_address.as_bytes());
    channel_id_msg.extend_from_slice(&1u64.to_be_bytes());
    let channel_id =
        multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&channel_id_msg);

    // Provide invalid signature
    let invalid_signature = [0u8; 64];

    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("1000000")
            .argument(&1u64.to_be_bytes()[..])
            .argument(&invalid_signature[..])
            .expect(TxExpect::err(10, "str:ed25519 verify error")),
    );
}

#[test]
fn test_negative_flow_insufficient_funds() {
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

    let mut channel_id_msg = Vec::new();
    channel_id_msg.extend_from_slice(employer_address.as_bytes());
    channel_id_msg.extend_from_slice(receiver_address.as_bytes());
    channel_id_msg.extend_from_slice(&1u64.to_be_bytes());
    let channel_id =
        multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&channel_id_msg);

    // Try to settle 6000000 when only 5000000 was deposited
    let amount = 6_000_000u64;
    let nonce_voucher = 1u64;

    let mut message = Vec::new();
    message.extend_from_slice(b"mpp-session-v1");
    message.extend_from_slice(sc_address.as_bytes());
    message.extend_from_slice(&channel_id);

    let mut amount_vec = amount.to_be_bytes().to_vec();
    while amount_vec.len() > 1 && amount_vec[0] == 0 {
        amount_vec.remove(0);
    }
    message.extend_from_slice(&amount_vec);
    message.extend_from_slice(&nonce_voucher.to_be_bytes());

    let hash = multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&message);
    let signature = signing_key.sign(hash.as_slice());

    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("6000000")
            .argument(&nonce_voucher.to_be_bytes()[..])
            .argument(signature.to_bytes().as_slice())
            .expect(TxExpect::user_error("str:Insufficient funds in session")),
    );
}
