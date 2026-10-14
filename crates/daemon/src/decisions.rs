//! Access to the OpenAI Decisions client for the single `flowspace3` binary.
//! The CLI already depends on this composition-root crate; re-exporting here
//! preserves the enforced crate graph, exactly as [`crate::github_copilot`]
//! does.

pub use fs3_providers::{
    Answer, ChoiceOption, ChoiceProbability, ChoiceValue, DECISIONS_DEFAULT_MODEL,
    DEFAULT_API_BASE, Decision, DecisionError, DecisionInput, DecisionRequest, DecisionsClient,
    ImageDetail, InputMessage, InputPart, Question, ScoreLevel, ScoreProbability, Usage,
};
