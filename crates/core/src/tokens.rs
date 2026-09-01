//! The one place fs3 counts tokens.
//!
//! Two callers need the count and they need it for opposite reasons, which is
//! exactly why they must not each grow their own:
//!
//! - The daemon's batch planner budgets the SUM of a request, to decide where
//!   to cut a merged call. Overestimating there splits a batch that would have
//!   fitted: cheap.
//! - [`fit_to_cap`] bounds a SINGLE input, to keep it inside the model's
//!   per-input cap. Underestimating there gets the input rejected — and since
//!   a retry re-sends the same bytes, rejected forever.
//!
//! # Why an estimate and not a tokenizer
//!
//! An exact count means shipping the model's own tokenizer. `cl100k_base`
//! would answer for Azure and OpenAI and be wrong for the local embedder,
//! which is `fastembed` running a WordPiece vocabulary — so "exact" would in
//! practice mean two counting mechanisms, one of which is applied to models it
//! does not describe. A single pessimistic estimate is honest about being an
//! estimate, and the safety margin below is sized so that being an estimate
//! does not matter.
//!
//! It lives in core rather than in the daemon that does the batching because
//! the fakes need it too: a fake embedder that rejects an oversized input has
//! to agree with the guard about what "oversized" means, or the tests prove
//! something other than the thing they claim.

/// Bytes per token, for the estimate.
///
/// The usual rule of thumb is four, derived from prose. Code tokenizes worse —
/// punctuation, identifiers and indentation all fragment — so three is the
/// pessimistic direction, and pessimistic is the safe direction for a BUDGET:
/// overestimating splits a batch that would have fitted, underestimating gets
/// the request rejected.
pub const BYTES_PER_TOKEN: usize = 3;

/// How much of a per-input cap [`fit_to_cap`] actually fills, as a fraction.
///
/// Two thirds, which is the same headroom the daemon's `TOKEN_BUDGET` takes
/// against Azure's 300k per-REQUEST ceiling, and for the same reason: the
/// number being compared is an ESTIMATE, and the cost of being wrong is a
/// whole call. Combined with [`BYTES_PER_TOKEN`] it means the guard assumes
/// **two bytes per token** rather than three.
///
/// That covers everything an index realistically holds. Code runs about 3.5
/// bytes per token, CJK text about 2.5 under a byte-level BPE. The content
/// that would still overflow is denser than two bytes per token — minified
/// bundles, base64 blobs, punctuation soup — and is named here rather than
/// pretended away: such an element fails exactly as it does today, visibly, in
/// the queue. Buying safety against it means truncating at one byte per token,
/// which is the only provable bound (a token is never fewer than one byte) and
/// would throw away three quarters of what every ordinary element could have
/// contributed.
const FILL: (usize, usize) = (2, 3);

/// A pessimistic token estimate for one text.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(BYTES_PER_TOKEN)
}

/// The conservative byte budget one input gets against `cap_tokens`.
///
/// This is the shared translation from a provider token cap to bytes. Callers
/// that split inputs must use it rather than multiplying by [`BYTES_PER_TOKEN`]
/// directly, or they silently discard the [`FILL`] safety margin.
///
/// Saturating, because a provider that truncates internally rather than
/// rejecting declares [`usize::MAX`] — a real value the port defines, not an
/// overflow waiting to panic in a debug build.
#[must_use]
pub fn input_budget_bytes(cap_tokens: usize) -> usize {
    cap_tokens
        .saturating_mul(FILL.0)
        .saturating_div(FILL.1)
        .saturating_mul(BYTES_PER_TOKEN)
}

/// The prefix of `text` that fits under a per-input token cap, or `None` when
/// the whole text already fits.
///
/// `None` rather than the text itself is the point: a caller cannot use this
/// and forget to record that truncation happened, because the truncated case
/// is the only one that hands anything back.
///
/// The cut lands on a UTF-8 character boundary — a byte-sliced string is not a
/// string, and Rust would panic rather than send it — and never on zero bytes
/// for a non-empty input, because an empty input is not an embedding and some
/// providers reject it outright.
#[must_use]
pub fn fit_to_cap(text: &str, cap_tokens: usize) -> Option<&str> {
    let budget = input_budget_bytes(cap_tokens);
    if text.len() <= budget {
        return None;
    }

    let mut end = budget.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        // A cap so small that its budget cannot hold one character. Absurd in
        // practice (the smallest model in play is 512 tokens), but the answer
        // to it is one character rather than nothing.
        end = text.chars().next().map_or(0, char::len_utf8);
    }

    Some(&text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cap that matters: `text-embedding-3-*` on Azure.
    const CAP: usize = 8192;

    #[test]
    fn a_text_within_the_budget_is_left_alone() {
        let text = "x".repeat(input_budget_bytes(CAP));
        assert_eq!(fit_to_cap(&text, CAP), None);
    }

    /// The defect this module exists for: 59 elements on a real index were
    /// rejected by Azure at 8192 tokens and retried into permanent failure.
    /// The estimate for what comes back must be under the cap with room to
    /// spare, because the estimate is all we have.
    #[test]
    fn an_oversized_text_comes_back_under_the_cap() {
        let text = "fn f() { let x = 1; }\n".repeat(4_000);
        let fitted = fit_to_cap(&text, CAP).expect("this text is over the cap");

        assert!(fitted.len() < text.len(), "it must actually shorten");
        assert!(
            estimate_tokens(fitted) <= CAP * FILL.0 / FILL.1,
            "a fitted text must sit inside the fill fraction, got {}",
            estimate_tokens(fitted)
        );
    }

    /// The margin is not decoration. Even if the real tokenizer is twice as
    /// hungry as [`BYTES_PER_TOKEN`] assumes — which is the CJK case — what
    /// comes back still fits.
    #[test]
    fn the_margin_survives_content_twice_as_dense_as_the_estimate() {
        let text = "。".repeat(40_000);
        let fitted = fit_to_cap(&text, CAP).expect("this text is over the cap");

        let pessimistic_tokens = fitted.len().div_ceil(2);
        assert!(
            pessimistic_tokens <= CAP,
            "at two bytes per token the fitted text is {pessimistic_tokens} tokens"
        );
    }

    /// Slicing a multi-byte character in half panics. The budget is a byte
    /// count, so the cut has to walk back to a boundary.
    #[test]
    fn the_cut_lands_on_a_character_boundary() {
        // Three bytes each, so a byte budget divisible by three is the only
        // one that lands cleanly — this cap's does not.
        let text = "é".repeat(20_000);
        let fitted = fit_to_cap(&text, CAP).expect("this text is over the cap");

        assert!(text.is_char_boundary(fitted.len()));
        assert!(fitted.chars().all(|c| c == 'é'));
    }

    #[test]
    fn a_non_empty_text_never_fits_to_nothing() {
        assert_eq!(fit_to_cap("hello", 0), Some("h"));
    }

    #[test]
    fn an_empty_text_is_already_within_any_cap() {
        assert_eq!(fit_to_cap("", CAP), None);
    }

    /// A provider that truncates internally declares [`usize::MAX`], and the
    /// budget arithmetic must answer that with "everything fits" rather than
    /// with an overflow panic.
    #[test]
    fn an_uncapped_provider_never_truncates() {
        let text = "x".repeat(10_000_000);
        assert_eq!(fit_to_cap(&text, usize::MAX), None);
    }
}
