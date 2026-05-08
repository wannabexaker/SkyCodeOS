// skycode-core must never import from skycode-inference.
use skycode_inference::inference::registry::ModelRegistry;

fn main() {
    let _ = std::mem::size_of::<ModelRegistry>();
}
