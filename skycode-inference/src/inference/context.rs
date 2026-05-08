#[derive(Debug, Clone)]
pub struct ContextWindow {
    pub max_tokens: usize,
    pub used: usize,
}

#[derive(Debug, Clone)]
pub struct ContextSlot {
    pub content: String,
    pub tokens: usize,
    pub priority: u8,
}

/// Fit context slots into the target window by removing the lowest-priority
/// slots first. If priorities are equal, larger slots are trimmed first.
pub fn fit_context(window: &ContextWindow, slots: &[ContextSlot]) -> Vec<ContextSlot> {
    if slots.is_empty() {
        return Vec::new();
    }

    let available = window.max_tokens.saturating_sub(window.used);

    let mut indexed: Vec<(usize, ContextSlot)> = slots.iter().cloned().enumerate().collect();
    let mut total_tokens: usize = indexed.iter().map(|(_, s)| s.tokens).sum();

    if total_tokens <= available {
        indexed.sort_by_key(|(idx, _)| *idx);
        return indexed.into_iter().map(|(_, s)| s).collect();
    }

    // Lowest priority first; for ties remove bigger slots first to free budget faster.
    indexed.sort_by(|(idx_a, a), (idx_b, b)| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| b.tokens.cmp(&a.tokens))
            .then_with(|| idx_a.cmp(idx_b))
    });

    while total_tokens > available && !indexed.is_empty() {
        if let Some((_, removed)) = indexed.first() {
            total_tokens = total_tokens.saturating_sub(removed.tokens);
        }
        indexed.remove(0);
    }

    indexed.sort_by_key(|(idx, _)| *idx);
    indexed.into_iter().map(|(_, s)| s).collect()
}
