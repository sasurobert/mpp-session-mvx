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
fn test_full_session_lifecycle() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    // Setup users
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
            .put_account(&receiver_address, Account::new().balance("0"))
            .new_address(&employer_address, 1, &sc_address)
            .block_timestamp_seconds(100),
    );

    // Deploy
    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    // Open Session
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

    // Calculate channel_id exactly as in SC
    let mut channel_id_msg = Vec::new();
    channel_id_msg.extend_from_slice(employer_address.as_bytes());
    channel_id_msg.extend_from_slice(receiver_address.as_bytes());
    let nonce: u64 = 1;
    channel_id_msg.extend_from_slice(&nonce.to_be_bytes());

    let channel_id =
        multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&channel_id_msg);

    // Prepare Voucher
    let amount = 10_000_000u64;
    let nonce_voucher = 1u64;

    // Construct message: "mpp-session-v1" + sc_address + channel_id + amount_be + nonce_be
    let mut message = Vec::new();
    message.extend_from_slice(b"mpp-session-v1");
    message.extend_from_slice(sc_address.as_bytes());
    message.extend_from_slice(&channel_id);

    // amount BigUint bytes (minimal BE)
    let amount_bytes = amount.to_be_bytes();
    let mut amount_vec = amount_bytes.to_vec();
    while amount_vec.len() > 1 && amount_vec[0] == 0 {
        amount_vec.remove(0);
    }
    message.extend_from_slice(&amount_vec);

    message.extend_from_slice(&nonce_voucher.to_be_bytes());

    let hash = multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&message);
    let signature = signing_key.sign(hash.as_slice());

    // Settle
    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("settle")
            .argument(&channel_id[..])
            .argument("10000000") // amount
            .argument(&nonce_voucher.to_be_bytes()[..])
            .argument(signature.to_bytes().as_slice()),
    );

    // Check balances after settle
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("10000000"))
            .put_account(
                &employer_address,
                CheckAccount::new().balance("5000000000000000000"),
            ),
    );

    // Close session with a final voucher
    let final_amount = 20_000_000u64;
    let final_nonce = 2u64;

    let mut final_msg = Vec::new();
    final_msg.extend_from_slice(b"mpp-session-v1");
    final_msg.extend_from_slice(sc_address.as_bytes());
    final_msg.extend_from_slice(&channel_id);

    let mut final_amount_vec = final_amount.to_be_bytes().to_vec();
    while final_amount_vec.len() > 1 && final_amount_vec[0] == 0 {
        final_amount_vec.remove(0);
    }
    final_msg.extend_from_slice(&final_amount_vec);
    final_msg.extend_from_slice(&final_nonce.to_be_bytes());

    let final_hash =
        multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&final_msg);
    let final_signature = signing_key.sign(final_hash.as_slice());

    world.sc_call(
        ScCallStep::new()
            .from(&receiver_address)
            .to(&sc_address)
            .function("close")
            .argument(&channel_id[..])
            .argument("20000000") // total authorized
            .argument(&final_nonce.to_be_bytes()[..])
            .argument(final_signature.to_bytes().as_slice()),
    );

    // Final balances:
    // Receiver: 20_000_000
    // Employer: 5 EGLD (unspent) + (5 EGLD - 0.00000002 EGLD) = 9.99999998 EGLD
    world.check_state_step(
        CheckStateStep::new()
            .put_account(&receiver_address, CheckAccount::new().balance("20000000"))
            .put_account(
                &employer_address,
                CheckAccount::new().balance("9999999999980000000"),
            ),
    );
}
