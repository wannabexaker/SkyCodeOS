// skycode-core must never import from skycode-agent.
// This fixture must FAIL to compile because boundary-tests only depends on
// skycode-core, which has no dep on skycode-agent.
use skycode_agent::agent::identity::CoderIdentity;

fn main() {
    let _ = std::mem::size_of::<CoderIdentity>();
}
