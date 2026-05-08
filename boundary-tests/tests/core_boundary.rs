/// Compile-time boundary enforcement for `skycode-core`.
///
/// `skycode-core` is the foundation crate — it must have zero dependencies on
/// any other workspace crate.  Each fixture below attempts an import that would
/// only compile if the forbidden dep were present.  trybuild asserts that every
/// fixture FAILS to compile, documenting and mechanically enforcing the boundary.
///
/// To regenerate `.stderr` files after a Rust toolchain upgrade:
///   TRYBUILD=overwrite cargo test -p boundary-tests
#[test]
fn phase6_crate_boundary_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/fixtures/core_uses_agent.rs");
    t.compile_fail("tests/fixtures/core_uses_tools.rs");
    t.compile_fail("tests/fixtures/core_uses_inference.rs");
    t.compile_fail("tests/fixtures/core_uses_orchestrator.rs");
}
