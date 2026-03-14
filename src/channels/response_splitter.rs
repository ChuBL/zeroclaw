use crate::config::MultiMessageConfig;
/// Compute a human-feeling delay (ms) before sending `part`.
///
/// The delay is length-weighted: longer text implies more "typing time",
/// biasing toward the upper end of the configured range. A random jitter
/// is added on top so identical-length parts never feel mechanical.
///
/// Formula:
///   t     = clamp(chars / REFERENCE_CHARS, 0.0, 1.0)   // 0 = short, 1 = long
///   mid   = lerp(min_ms, max_ms, t)                     // length-weighted midpoint
///   delay = clamp(mid + jitter, min_ms, max_ms)         // add noise, clamp to range
pub fn human_delay_ms(part: &str, min_ms: u64, max_ms: u64) -> u64 {
    if min_ms >= max_ms {
        return min_ms;
    }
    // A "reference" length (chars) that fully saturates to max_ms (~60 chars = medium sentence).
    const REFERENCE_CHARS: f64 = 60.0;

    let chars = part.chars().count() as f64;
    let t = (chars / REFERENCE_CHARS).clamp(0.0, 1.0);

    let range = (max_ms - min_ms) as f64;
    let mid = min_ms as f64 + t * range;

    // Jitter: ±25 % of full range. Map random u64 → [-1.0, 1.0] → scale.
    let jitter_range = range * 0.25;
    let noise = (rand::random::<u64>() as f64 / u64::MAX as f64) * 2.0 - 1.0; // [-1.0, 1.0]
    let jitter = noise * jitter_range;

    let delay = (mid + jitter).clamp(min_ms as f64, max_ms as f64);
    delay.round() as u64
}

/// Splits `content` by `config.break_marker`, trims each part, filters empties,
/// and optionally coalesces short adjacent segments.
///
/// Returns a single-element `Vec` containing the original content when:
/// - `break_marker` is empty (feature disabled), or
/// - the marker does not appear in the content.
pub fn split_response(content: &str, config: &MultiMessageConfig) -> Vec<String> {
    if config.break_marker.is_empty() {
        return vec![content.to_string()];
    }

    let parts: Vec<String> = content
        .split(config.break_marker.as_str())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .collect();

    if parts.is_empty() {
        // Edge case: response was only the marker; return the trimmed original.
        let trimmed = content.trim().to_string();
        return if trimmed.is_empty() {
            vec![content.to_string()]
        } else {
            vec![trimmed]
        };
    }

    if config.coalesce_short_messages {
        coalesce(parts, config.coalesce_min_chars)
    } else {
        parts
    }
}

/// Merges segments shorter than `min_chars` into the previous segment.
/// A short leading segment is kept as-is until a long segment follows it.
fn coalesce(parts: Vec<String>, min_chars: usize) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for part in parts {
        // Use char count (not byte length) so CJK and emoji are counted correctly.
        if part.chars().count() < min_chars {
            if let Some(last) = result.last_mut() {
                last.push('\n');
                last.push_str(&part);
                continue;
            }
        }
        result.push(part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MultiMessageConfig;

    fn cfg(marker: &str, coalesce: bool) -> MultiMessageConfig {
        MultiMessageConfig {
            break_marker: marker.into(),
            human_delay_ms: 0,
            coalesce_short_messages: coalesce,
            coalesce_min_chars: 10,
        }
    }

    #[test]
    fn disabled_returns_original() {
        let c = cfg("", false);
        assert_eq!(split_response("hello world", &c), vec!["hello world"]);
    }

    #[test]
    fn no_marker_in_content_returns_original() {
        let c = cfg("[BREAK]", false);
        assert_eq!(split_response("hello world", &c), vec!["hello world"]);
    }

    #[test]
    fn splits_on_marker() {
        let c = cfg("[BREAK]", false);
        assert_eq!(
            split_response("hello[BREAK]world", &c),
            vec!["hello", "world"]
        );
    }

    #[test]
    fn trims_whitespace_around_parts() {
        let c = cfg("[BREAK]", false);
        assert_eq!(
            split_response("  hello  [BREAK]  world  ", &c),
            vec!["hello", "world"]
        );
    }

    #[test]
    fn filters_empty_segments() {
        let c = cfg("[BREAK]", false);
        assert_eq!(
            split_response("[BREAK]hello[BREAK][BREAK]world[BREAK]", &c),
            vec!["hello", "world"]
        );
    }

    #[test]
    fn coalesces_short_leading_segment() {
        let c = cfg("[BREAK]", true);
        // "hi" is < 10 chars; it gets merged with the next
        assert_eq!(
            split_response("hi[BREAK]this is a longer segment", &c),
            vec!["hi\nthis is a longer segment"]
        );
    }

    #[test]
    fn does_not_coalesce_long_segments() {
        let c = cfg("[BREAK]", true);
        assert_eq!(
            split_response("first long part here[BREAK]second long part here", &c),
            vec!["first long part here", "second long part here"]
        );
    }

    #[test]
    fn only_marker_returns_original() {
        let c = cfg("[BREAK]", false);
        let result = split_response("[BREAK]", &c);
        assert_eq!(result.len(), 1);
    }
}
