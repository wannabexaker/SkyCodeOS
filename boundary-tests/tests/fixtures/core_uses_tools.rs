// skycode-core must never import from skycode-tools.
use skycode_tools::tools::apply::ApplyError;

fn main() {
    let _ = std::mem::size_of::<ApplyError>();
}
