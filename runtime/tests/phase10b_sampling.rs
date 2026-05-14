//! Phase 10B - Extended sampling parameters.
//!
//! Verifies:
//! 1. Existing profiles parse with no sampling extras.
//! 2. The new `experimental` profile parses with extras populated.
//! 3. SamplingExtras::default() produces all-None.
//! 4. ChatCompletionRequest serialization omits unset extras.
//! 5. ChatCompletionRequest serialization includes set extras.

use std::path::{Path, PathBuf};

use serde_json::Value;
use skycode_runtime::agent::profile::load_profile;
use skycode_runtime::inference::{chat_completion_request_json, SamplingExtras};

#[test]
fn phase10b_precise_profile_has_no_sampling_extras() -> Result<(), Box<dyn std::error::Error>> {
    let profile = load_profile(&agents_root(), "precise")?;

    assert_eq!(profile.top_k, None);
    assert_eq!(profile.top_p, None);
    assert_eq!(profile.min_p, None);
    assert_eq!(profile.typical_p, None);
    assert_eq!(profile.repeat_last_n, None);
    assert_eq!(profile.presence_penalty, None);
    assert_eq!(profile.frequency_penalty, None);
    assert_eq!(profile.dynatemp_range, None);
    assert_eq!(profile.dynatemp_exponent, None);
    assert_eq!(profile.dry_multiplier, None);
    assert_eq!(profile.dry_base, None);
    assert_eq!(profile.dry_allowed_length, None);
    assert_eq!(profile.dry_penalty_last_n, None);
    assert_eq!(profile.xtc_probability, None);
    assert_eq!(profile.xtc_threshold, None);

    Ok(())
}

#[test]
fn phase10b_experimental_profile_loads_full_sampling_set() -> Result<(), Box<dyn std::error::Error>>
{
    let profile = load_profile(&agents_root(), "experimental")?;

    assert_eq!(profile.top_k, Some(50));
    assert_f32_option(profile.top_p, 0.92)?;
    assert_f32_option(profile.min_p, 0.05)?;
    assert_eq!(profile.repeat_last_n, Some(128));
    assert_f32_option(profile.presence_penalty, 0.1)?;
    assert_f32_option(profile.frequency_penalty, 0.1)?;
    assert_f32_option(profile.dry_multiplier, 0.6)?;
    assert_f32_option(profile.dry_base, 1.75)?;
    assert_eq!(profile.dry_allowed_length, Some(2));

    assert_eq!(profile.typical_p, None);
    assert_eq!(profile.dynatemp_range, None);
    assert_eq!(profile.dynatemp_exponent, None);
    assert_eq!(profile.dry_penalty_last_n, None);
    assert_eq!(profile.xtc_probability, None);
    assert_eq!(profile.xtc_threshold, None);

    Ok(())
}

#[test]
fn phase10b_sampling_extras_default_is_all_none() {
    let extras = SamplingExtras::default();

    assert_eq!(extras.top_k, None);
    assert_eq!(extras.top_p, None);
    assert_eq!(extras.min_p, None);
    assert_eq!(extras.typical_p, None);
    assert_eq!(extras.repeat_last_n, None);
    assert_eq!(extras.presence_penalty, None);
    assert_eq!(extras.frequency_penalty, None);
    assert_eq!(extras.dynatemp_range, None);
    assert_eq!(extras.dynatemp_exponent, None);
    assert_eq!(extras.dry_multiplier, None);
    assert_eq!(extras.dry_base, None);
    assert_eq!(extras.dry_allowed_length, None);
    assert_eq!(extras.dry_penalty_last_n, None);
    assert_eq!(extras.xtc_probability, None);
    assert_eq!(extras.xtc_threshold, None);
}

#[test]
fn phase10b_chat_completion_request_omits_unset_sampling_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let value =
        chat_completion_request_json("hello", 0.1, 1024, 1.1, None, SamplingExtras::default())?;
    let object = value
        .as_object()
        .ok_or("chat completion request must serialize as an object")?;

    for key in sampling_field_names() {
        assert!(
            !object.contains_key(key),
            "request unexpectedly contained {key}"
        );
    }

    Ok(())
}

#[test]
fn phase10b_chat_completion_request_includes_set_sampling_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let value = chat_completion_request_json(
        "hello",
        0.1,
        1024,
        1.1,
        None,
        SamplingExtras {
            top_k: Some(50),
            top_p: Some(0.9),
            ..Default::default()
        },
    )?;

    assert_eq!(value.get("top_k").and_then(Value::as_u64), Some(50));
    let top_p = value
        .get("top_p")
        .and_then(Value::as_f64)
        .ok_or("top_p missing from serialized request")?;
    assert!((top_p - 0.9).abs() < 0.000_001);
    assert!(value.get("min_p").is_none());

    Ok(())
}

fn agents_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("agents")
}

fn assert_f32_option(value: Option<f32>, expected: f32) -> Result<(), Box<dyn std::error::Error>> {
    let value = value.ok_or("expected Some(f32)")?;
    assert!((value - expected).abs() < f32::EPSILON);
    Ok(())
}

fn sampling_field_names() -> [&'static str; 15] {
    [
        "top_k",
        "top_p",
        "min_p",
        "typical_p",
        "repeat_last_n",
        "presence_penalty",
        "frequency_penalty",
        "dynatemp_range",
        "dynatemp_exponent",
        "dry_multiplier",
        "dry_base",
        "dry_allowed_length",
        "dry_penalty_last_n",
        "xtc_probability",
        "xtc_threshold",
    ]
}
