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
fn test_open_zero_deposit() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

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

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 10000u64;
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("0") // zero deposit
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..])
            .expect(TxExpect::user_error("str:Zero deposit not allowed")),
    );
}

#[test]
fn test_open_deadline_in_past() {
    let mut world = world();
    let root = "file:output/mpp-session-mvx.wasm";

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
            .block_timestamp_seconds(5000), // current time is 5000
    );

    world.sc_deploy(ScDeployStep::new().from(&employer_address).code(root));

    let deadline = 100u64; // deadline in the past (100 < 5000)
    world.sc_call(
        ScCallStep::new()
            .from(&employer_address)
            .to(&sc_address)
            .egld_value("1000000000000000000")
            .function("open")
            .argument(receiver_address.as_array().as_slice())
            .argument(&deadline.to_be_bytes()[..])
            .expect(TxExpect::user_error("str:Deadline must be in the future")),
    );
}
