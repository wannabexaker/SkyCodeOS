//! GPU hardware detection module.
//! Detects available GPU devices via nvidia-smi (NVIDIA) or DXGI (Windows fallback).
//! Never panics. Returns empty Vec on CPU-only machines or when detection fails.

use std::process::Command;

/// GPU information record.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuInfo {
    /// Zero-indexed GPU enumeration position.
    pub index: usize,
    /// GPU model name (e.g., "NVIDIA GeForce RTX 3060").
    pub name: String,
    /// Total video memory in MB.
    pub vram_total_mb: u64,
    /// Free video memory in MB.
    pub vram_free_mb: u64,
}

/// Detect available GPU hardware.
///
/// Returns empty Vec on CPU-only machines or when detection fails.
/// Never panics. Never returns an error.
///
/// Detection order:
/// 1. NVIDIA via nvidia-smi XML output
/// 2. Windows DXGI adapter enumeration (if nvidia-smi fails and on Windows)
/// 3. Empty Vec fallback
pub fn detect_gpus() -> Vec<GpuInfo> {
    // Step 1: Try NVIDIA via nvidia-smi
    if let Some(gpus) = detect_gpus_nvidia() {
        if !gpus.is_empty() {
            return gpus;
        }
    }

    // Step 2: Windows DXGI fallback (CPU-only machines will skip this)
    #[cfg(target_os = "windows")]
    {
        if let Some(gpus) = detect_gpus_dxgi() {
            if !gpus.is_empty() {
                return gpus;
            }
        }
    }

    // Step 3: Fallback
    Vec::new()
}

/// Detect NVIDIA GPUs via nvidia-smi XML output.
///
/// Spawns `nvidia-smi -q -x` and parses XML. Returns None if command fails,
/// parsing fails, or no GPUs found.
fn detect_gpus_nvidia() -> Option<Vec<GpuInfo>> {
    // Spawn nvidia-smi with XML output
    let output = Command::new("nvidia-smi")
        .arg("-q")
        .arg("-x")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let xml_text = String::from_utf8_lossy(&output.stdout);

    // Parse XML and extract GPU data
    parse_nvidia_xml(&xml_text)
}

/// Parse NVIDIA XML output from nvidia-smi.
///
/// Expected structure:
/// ```xml
/// <nvidia_smi_log>
///   <gpu id="0">
///     <product_name>NVIDIA GeForce RTX 3060</product_name>
///     <fb_memory_usage>
///       <total>12288 MiB</total>
///       <free>10240 MiB</free>
///     </fb_memory_usage>
///   </gpu>
/// </nvidia_smi_log>
/// ```
fn parse_nvidia_xml(xml_text: &str) -> Option<Vec<GpuInfo>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml_text);
    let mut buf = Vec::new();

    let mut gpus = Vec::new();
    let mut current_gpu_index = 0;
    let mut current_gpu_name = String::new();
    let mut current_gpu_total_mb: Option<u64> = None;
    let mut current_gpu_free_mb: Option<u64> = None;

    let mut in_gpu = false;
    let mut in_product_name = false;
    let mut in_total = false;
    let mut in_free = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name_bytes = e.local_name();
                let tag_name = std::str::from_utf8(tag_name_bytes.as_ref()).unwrap_or("");
                match tag_name {
                    "gpu" => {
                        in_gpu = true;
                        current_gpu_index = gpus.len();
                        current_gpu_name.clear();
                        current_gpu_total_mb = None;
                        current_gpu_free_mb = None;
                    }
                    "product_name" if in_gpu => in_product_name = true,
                    "total" if in_gpu => in_total = true,
                    "free" if in_gpu => in_free = true,
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name_bytes = e.local_name();
                let tag_name = std::str::from_utf8(tag_name_bytes.as_ref()).unwrap_or("");
                match tag_name {
                    "gpu" if in_gpu => {
                        if !current_gpu_name.is_empty() {
                            if let (Some(total), Some(free)) =
                                (current_gpu_total_mb, current_gpu_free_mb)
                            {
                                gpus.push(GpuInfo {
                                    index: current_gpu_index,
                                    name: current_gpu_name.clone(),
                                    vram_total_mb: total,
                                    vram_free_mb: free,
                                });
                            }
                        }
                        in_gpu = false;
                    }
                    "product_name" if in_product_name => in_product_name = false,
                    "total" if in_total => in_total = false,
                    "free" if in_free => in_free = false,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    let text_str = text.as_ref();
                    if in_product_name {
                        current_gpu_name = text_str.to_string();
                    } else if in_total {
                        // Parse "12288 MiB" -> 12288
                        if let Some(mb_value) = parse_memory_string(text_str) {
                            current_gpu_total_mb = Some(mb_value);
                        }
                    } else if in_free {
                        // Parse "10240 MiB" -> 10240
                        if let Some(mb_value) = parse_memory_string(text_str) {
                            current_gpu_free_mb = Some(mb_value);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

/// Parse memory string like "12288 MiB" into u64 MB value.
fn parse_memory_string(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    parts[0].parse::<u64>().ok()
}

/// Detect GPUs via Windows DXGI adapter enumeration.
///
/// Only compiled on Windows via `#[cfg(target_os = "windows")]`.
#[cfg(target_os = "windows")]
fn detect_gpus_dxgi() -> Option<Vec<GpuInfo>> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory, IDXGIFactory};

    // Create DXGI factory
    let factory: IDXGIFactory = unsafe { CreateDXGIFactory().ok()? };

    let mut gpus = Vec::new();
    let mut index = 0;

    // Enumerate adapters
    loop {
        match unsafe { factory.EnumAdapters(index) } {
            Ok(adapter) => {
                if let Ok(desc) = unsafe { adapter.GetDesc() } {
                    // Skip adapters with zero dedicated video memory (software adapters)
                    if desc.DedicatedVideoMemory == 0 {
                        index += 1;
                        continue;
                    }

                    // Convert VRAM from bytes to MB (ensure result is u64)
                    let vram_total_mb = (desc.DedicatedVideoMemory as u64) / (1024u64 * 1024u64);

                    // Convert description from UTF-16 to String
                    let name = String::from_utf16_lossy(&desc.Description)
                        .trim_end_matches('\0')
                        .to_string();

                    gpus.push(GpuInfo {
                        index: gpus.len(),
                        name,
                        vram_total_mb,
                        vram_free_mb: vram_total_mb, // DXGI doesn't expose free VRAM
                    });
                }
                index += 1;
            }
            Err(_) => break, // No more adapters
        }
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_string() {
        assert_eq!(parse_memory_string("12288 MiB"), Some(12288));
        assert_eq!(parse_memory_string("1024 MiB"), Some(1024));
        assert_eq!(parse_memory_string("0 MiB"), Some(0));
        assert_eq!(parse_memory_string("invalid"), None);
        assert_eq!(parse_memory_string(""), None);
    }
}
