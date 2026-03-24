use multiversx_sc_scenario::*;

fn world() -> ScenarioWorld {
    let mut blockchain = ScenarioWorld::new();
    // blockchain.register_contract("file:output/mpp-session-mvx.wasm", mpp_session_mvx::ContractBuilder);
    blockchain
}

#[test]
fn test_dummy() {
    let mut _world = world();
}
