use multiversx_sc_scenario::imports::*;
use multiversx_sc::types::Address;
use multiversx_sc_scenario::scenario_model::*;

fn world() -> ScenarioWorld {
    let mut blockchain = ScenarioWorld::new();
    blockchain.register_contract(
        "file:output/mpp-session-mvx.wasm",
        mpp_session_mvx::ContractBuilder,
    );
    blockchain
}

fn compute_channel_id(employer: &Address, receiver: &Address) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(employer.as_bytes());
    msg.extend_from_slice(receiver.as_bytes());
    msg.extend_from_slice(&1u64.to_be_bytes());
    multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&msg).to_vec()
}

#[test]
fn test_request_close_not_employer() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    let employer_address = Address::from([1u8; 32]);
    let receiver_address = Address::from([2u8; 32]);
    let attacker_address = Address::from([3u8; 32]);
    let sc_address = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4]);

    world.set_state_step(
        SetStateStep::new()
            .put_account(&employer_address, Account::new().balance("10000000000000000000").nonce(1))
            .put_account(&receiver_address, Account::new().balance("0"))
            .put_account(&attacker_address, Account::new().balance("100000000000000000").nonce(1))
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

    // Fast forward past deadline
    world.set_state_step(SetStateStep::new().block_timestamp_seconds(10001));

    // Attacker (not employer) tries to request_close
    world.sc_call(
        ScCallStep::new()
            .from(&attacker_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..])
            .expect(TxExpect::user_error("str:Only employer can request close")),
    );
}

#[test]
fn test_request_close_already_closed() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

    let employer_address = Address::from([1u8; 32]);
    let receiver_address = Address::from([2u8; 32]);
    let sc_address = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3]);

    world.set_state_step(
        SetStateStep::new()
            .put_account(&employer_address, Account::new().balance("10000000000000000000").nonce(1))
            .put_account(&receiver_address, Account::new().balance("0"))
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

    // Close successfully
    world.set_state_step(SetStateStep::new().block_timestamp_seconds(10001));
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..]),
    );

    // Try to close again
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..])
            .expect(TxExpect::user_error("str:Session already closed")),
    );
}
