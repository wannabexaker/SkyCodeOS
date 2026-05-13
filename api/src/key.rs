use std::path::Path;

/// Load the API key from `.skycode/api.key`.
/// Creates the file with a fresh random key if it does not exist.
/// Returns the key as a lowercase hex string (64 chars).
pub fn load_or_create(project_root: &Path) -> Result<String, std::io::Error> {
    let key_dir = project_root.join(".skycode");
    std::fs::create_dir_all(&key_dir)?;
    let key_path = key_dir.join("api.key");

    if key_path.exists() {
        let key = std::fs::read_to_string(&key_path)?;
        return Ok(key.trim().to_string());
    }

    // Generate 32 random bytes via the OS RNG
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(&key_path, &hex)?;
    Ok(hex)
}
