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

/// Helper: deploy + open a session, return channel_id
fn setup_open_session(
    world: &mut ScenarioWorld,
    employer_address: &Address,
    receiver_address: &Address,
    sc_address: &Address,
) -> Vec<u8> {
    let root = "file:output/mpp-session-mvx.wasm";
    world.sc_deploy(ScDeployStep::new().from(employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(employer_address)
            .to(sc_address)
            .egld_value("5000000000000000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..]),
    );

    let mut channel_id_msg = Vec::new();
    channel_id_msg.extend_from_slice(employer_address.as_bytes());
    channel_id_msg.extend_from_slice(receiver_address.as_bytes());
    channel_id_msg.extend_from_slice(&1u64.to_be_bytes());
    multiversx_sc_scenario::multiversx_chain_vm::crypto_functions::keccak256(&channel_id_msg)
        .to_vec()
}

#[test]
fn test_top_up_happy_path() {
    let mut world = world();

    let employer_address = Address::from([1u8; 32]);
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

    let channel_id = setup_open_session(
        &mut world,
        &employer_address,
        &receiver_address,
        &sc_address,
    );

    // Top up with 1 more EGLD
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("1000000000000000000")
            .function("top_up")
            .argument(&channel_id[..]),
    );

    // Verify SC holds 6 EGLD (5 + 1)
    world.check_state_step(CheckStateStep::new().put_account(
        &sc_address,
        CheckAccount::new().balance("6000000000000000000"),
    ));
}

#[test]
fn test_top_up_closed_session() {
    let mut world = world();

    let employer_address = Address::from([1u8; 32]);
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

    let channel_id = setup_open_session(
        &mut world,
        &employer_address,
        &receiver_address,
        &sc_address,
    );

    // Fast forward past deadline and close
    world.set_state_step(SetStateStep::new().block_timestamp_seconds(10001));
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .function("request_close")
            .argument(&channel_id[..]),
    );

    // Try to top up closed session
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("1000000000000000000")
            .function("top_up")
            .argument(&channel_id[..])
            .expect(TxExpect::user_error("str:Session already closed")),
    );
}

#[test]
fn test_top_up_zero_amount() {
    let mut world = world();

    let employer_address = Address::from([1u8; 32]);
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

    let channel_id = setup_open_session(
        &mut world,
        &employer_address,
        &receiver_address,
        &sc_address,
    );

    // Top up with zero
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("0")
            .function("top_up")
            .argument(&channel_id[..])
            .expect(TxExpect::user_error("str:Zero deposit not allowed")),
    );
}
