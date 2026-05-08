// skycode-core must never import from skycode-orchestrator.
use skycode_orchestrator::orchestrator::router::TaskClass;

fn main() {
    let _ = std::mem::size_of::<TaskClass>();
}
