//! `flowspace3 decision` — one bounded question about text or images, answered
//! by OpenAI's Decisions API.
//!
//! A direct call: no daemon, no database. The verb turns its flags into one
//! `POST /v1/decisions` and the answer into one envelope. The key is read from
//! the environment (or `secrets.env`, which boot loads into it) by NAME, the
//! same way every provider instance reads one — never from a flag value.

use std::io::Read as _;
use std::path::Path;
use std::time::Instant;

use fs3_core::{
    CostEstimate, DecisionsConfig, Effective, ProviderInstance, catalog,
    envelope::{Envelope, Failure},
};
use fs3_daemon::decisions::{
    Answer, ChoiceOption, ChoiceValue, DEFAULT_API_BASE, DecisionError, DecisionInput,
    DecisionRequest, DecisionsClient, ImageDetail, InputMessage, InputPart, Question, ScoreLevel,
    Usage,
};
use serde::{Deserialize, Serialize};

/// The verb's name on the envelope.
pub const COMMAND: &str = "decision";

/// `--detail`, as the command line spells it.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum DetailArg {
    Low,
    High,
    Auto,
    Original,
}

impl From<DetailArg> for ImageDetail {
    fn from(detail: DetailArg) -> Self {
        match detail {
            DetailArg::Low => Self::Low,
            DetailArg::High => Self::High,
            DetailArg::Auto => Self::Auto,
            DetailArg::Original => Self::Original,
        }
    }
}

/// Everything the command line said.
#[derive(Debug, Default)]
pub struct DecisionArgs {
    /// The question's instructions. Optional only when `questions` is given.
    pub prompt: Option<String>,
    pub name: Option<String>,
    /// `VALUE[=DESCRIPTION]`: makes the prompt a `choice` question.
    pub choices: Vec<String>,
    /// `LABEL[=DESCRIPTION]`, lowest first: makes the prompt a `score` question.
    pub levels: Vec<String>,
    /// A JSON file (`-` for stdin) of further questions in the API's shape.
    pub questions: Option<String>,
    /// Text evidence; `-` reads stdin.
    pub inputs: Vec<String>,
    /// Image files, image URLs, or text files.
    pub attachments: Vec<String>,
    pub detail: Option<DetailArg>,
    pub model: String,
    pub provider: Option<String>,
    pub api_key_env: Option<String>,
    pub safety_identifier: Option<String>,
}

/// What the input carried, without echoing it: an inlined image is megabytes
/// of base64 nobody wants back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSummary {
    pub texts: usize,
    pub images: usize,
    /// Each `--attach`, as given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
}

/// The `data` of a `decision` envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionReport {
    pub model: String,
    /// The questions as sent, so each answer can be read next to what it
    /// answers.
    pub questions: Vec<Question>,
    /// One per question, in question order.
    pub answers: Vec<Answer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    pub input: InputSummary,
    /// The API base that answered.
    pub endpoint: String,
    pub latency_ms: u64,
    /// Usage times the `[decisions.prices]` table — an estimate, never a
    /// bill. Absent when the model has no price or the service sent no usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostEstimate>,
}

/// Run the verb. Every outcome is an envelope.
pub async fn run(args: DecisionArgs, config_dir: &Path) -> Envelope<DecisionReport> {
    match decide(args, config_dir).await {
        Ok(envelope) => envelope,
        Err(failure) => {
            let next = failure.fix.clone();
            Envelope::failed(COMMAND, failure).with_next_action(next)
        }
    }
}

async fn decide(
    args: DecisionArgs,
    config_dir: &Path,
) -> Result<Envelope<DecisionReport>, Failure> {
    let mut stdin = Stdin::default();
    let questions = build_questions(&args, &mut stdin)?;
    let (input, summary) = build_input(&args, &mut stdin)?;
    // A broken config only matters to `--provider`; prices fall back to the
    // built-in table rather than failing a call that never needed config.
    let effective = crate::settings::load_effective_from(config_dir);
    let endpoint = resolve_endpoint(&args, effective.as_ref())?;
    let prices = effective.as_ref().map_or_else(
        |_| DecisionsConfig::default(),
        |effective| effective.config.decisions.clone(),
    );

    let request = DecisionRequest {
        model: args.model,
        input,
        questions,
        safety_identifier: args.safety_identifier,
    };
    let client = DecisionsClient::new(Some(endpoint.api_base.clone()), endpoint.api_key);
    let started = Instant::now();
    let decision = client
        .create(&request)
        .await
        .map_err(|error| failure_of(&error, &endpoint.key_env))?;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let cost = decision.usage.as_ref().and_then(|usage| {
        prices
            .prices
            .get(&decision.model)
            .or_else(|| prices.prices.get(&request.model))
            .map(|price| price.estimate(usage.input_tokens, usage.output_tokens))
    });

    let next = next_action(&decision.answers);
    Ok(Envelope::ok(
        COMMAND,
        DecisionReport {
            model: decision.model,
            questions: request.questions,
            answers: decision.answers,
            usage: decision.usage,
            input: summary,
            endpoint: endpoint.api_base,
            latency_ms,
            cost,
        },
    )
    .with_next_action(next))
}

fn usage(message: impl Into<String>) -> Failure {
    Failure::new(&catalog::USAGE_INVALID, message).with_fix(
        "run `flowspace3 decision --help`; `flowspace3 docs get decisions` has worked examples",
    )
}

/// Stdin can be read once. Two flags that both say `-` are a usage error,
/// rather than one of them silently getting nothing.
#[derive(Default)]
struct Stdin {
    claimed_by: Option<&'static str>,
}

impl Stdin {
    fn read(&mut self, flag: &'static str) -> Result<String, Failure> {
        if let Some(first) = self.claimed_by {
            return Err(usage(format!(
                "`{first} -` already reads stdin; `{flag} -` cannot read it too"
            )));
        }
        self.claimed_by = Some(flag);
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| usage(format!("`{flag} -`: stdin is not readable text: {error}")))?;
        Ok(text)
    }
}

/// `VALUE=DESCRIPTION` → both halves; a bare value has no description. Split
/// at the FIRST `=`, so a description may contain more of them.
fn split_spec(spec: &str) -> (String, Option<String>) {
    match spec.split_once('=') {
        Some((value, description)) if !description.trim().is_empty() => (
            value.trim().to_string(),
            Some(description.trim().to_string()),
        ),
        Some((value, _)) => (value.trim().to_string(), None),
        None => (spec.trim().to_string(), None),
    }
}

/// The prompt's own question first, then any from `--questions`.
fn build_questions(args: &DecisionArgs, stdin: &mut Stdin) -> Result<Vec<Question>, Failure> {
    let mut questions = Vec::new();

    if let Some(prompt) = &args.prompt {
        let instructions = prompt.trim().to_string();
        if instructions.is_empty() {
            return Err(usage("the prompt is blank: it is the question being asked"));
        }
        let name = args.name.clone();
        questions.push(if !args.choices.is_empty() {
            Question::Choice {
                name,
                instructions,
                choices: args
                    .choices
                    .iter()
                    .map(|spec| {
                        let (value, description) = split_spec(spec);
                        ChoiceOption {
                            value: ChoiceValue::Text(value),
                            description,
                        }
                    })
                    .collect(),
            }
        } else if !args.levels.is_empty() {
            Question::Score {
                name,
                instructions,
                levels: args
                    .levels
                    .iter()
                    .map(|spec| {
                        let (label, description) = split_spec(spec);
                        ScoreLevel { label, description }
                    })
                    .collect(),
            }
        } else {
            Question::Predicate { name, instructions }
        });
    }

    if let Some(source) = &args.questions {
        let text = if source == "-" {
            stdin.read("--questions")?
        } else {
            std::fs::read_to_string(source)
                .map_err(|error| usage(format!("--questions {source}: {error}")))?
        };
        questions.extend(parse_questions(&text).map_err(|error| {
            usage(format!(
                "--questions {source}: not a question or an array of questions in the API's shape: {error}"
            ))
        })?);
    }

    if questions.is_empty() {
        return Err(usage(
            "nothing to ask: give a prompt, or --questions <file>",
        ));
    }
    Ok(questions)
}

/// One question object, or an array of them.
fn parse_questions(text: &str) -> serde_json::Result<Vec<Question>> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<Question>),
        One(Question),
    }
    Ok(match serde_json::from_str(text)? {
        OneOrMany::Many(questions) => questions,
        OneOrMany::One(question) => vec![question],
    })
}

/// The image types the endpoint reads, by extension.
fn image_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// The evidence: `--input` texts, then `--attach`ments, in the order given.
///
/// With no evidence at all the prompt is judged on its own — `flowspace3
/// decision "Is Rust memory safe?"` is a complete question.
fn build_input(
    args: &DecisionArgs,
    stdin: &mut Stdin,
) -> Result<(DecisionInput, InputSummary), Failure> {
    let detail = args.detail.map(ImageDetail::from);
    let mut parts = Vec::new();
    let mut summary = InputSummary {
        attachments: args.attachments.clone(),
        ..InputSummary::default()
    };

    for text in &args.inputs {
        let text = if text == "-" {
            stdin.read("--input")?
        } else {
            text.clone()
        };
        parts.push(InputPart::text(text));
        summary.texts += 1;
    }

    for attachment in &args.attachments {
        if attachment.starts_with("https://") || attachment.starts_with("http://") {
            parts.push(InputPart::image_url(attachment.clone(), detail));
            summary.images += 1;
            continue;
        }
        let path = Path::new(attachment);
        let bytes = std::fs::read(path)
            .map_err(|error| usage(format!("--attach {attachment}: {error}")))?;
        if let Some(mime) = image_mime(path) {
            parts.push(InputPart::image_bytes(mime, &bytes, detail));
            summary.images += 1;
        } else {
            let text = String::from_utf8(bytes).map_err(|_| {
                usage(format!(
                    "--attach {attachment}: neither an image (png, jpg, gif, webp) nor UTF-8 text"
                ))
            })?;
            parts.push(InputPart::text(format!(
                "Attached file `{attachment}`:\n\n{text}"
            )));
            summary.texts += 1;
        }
    }

    let input = if parts.is_empty() {
        let prompt = args
            .prompt
            .clone()
            .ok_or_else(|| usage("nothing to decide about: pass --input or --attach"))?;
        summary.texts = 1;
        DecisionInput::Text(prompt)
    } else if let [InputPart::InputText { text }] = parts.as_slice() {
        DecisionInput::Text(text.clone())
    } else {
        DecisionInput::Messages(vec![InputMessage { content: parts }])
    };
    Ok((input, summary))
}

/// Where the request goes, and with which key.
struct Endpoint {
    api_base: String,
    api_key: String,
    /// The variable the key came from, for messages. Never the key.
    key_env: String,
}

/// The key variables tried, in order, when nothing names one: the convention,
/// then the spelling Jordan's `.env` uses.
pub const DEFAULT_KEY_ENVS: &[&str] = &[ProviderInstance::DEFAULT_API_KEY_ENV, "OPEN_AI"];

/// The first of [`DEFAULT_KEY_ENVS`] holding a non-blank value, or the
/// conventional name when none does — so the "not set" message names it.
fn default_key_env(lookup: impl Fn(&str) -> Option<String>) -> &'static str {
    DEFAULT_KEY_ENVS
        .iter()
        .copied()
        .find(|name| lookup(name).is_some_and(|value| !value.trim().is_empty()))
        .unwrap_or(DEFAULT_KEY_ENVS[0])
}

/// `--provider` borrows a configured instance's base and key variable;
/// without one, OpenAI itself and the first of [`DEFAULT_KEY_ENVS`] that is
/// set. `--api-key-env` names a different variable either way.
fn resolve_endpoint(
    args: &DecisionArgs,
    effective: Result<&Effective, &anyhow::Error>,
) -> Result<Endpoint, Failure> {
    let (api_base, configured_env) = match &args.provider {
        None => (
            DEFAULT_API_BASE.to_string(),
            Some(default_key_env(|name| std::env::var(name).ok()).to_string()),
        ),
        Some(name) => {
            let effective = effective
                .map_err(|error| Failure::new(&catalog::CONFIG_INVALID, format!("{error:#}")))?;
            let instance = effective.config.provider(name).map_err(|error| {
                Failure::new(&catalog::CONFIG_PROVIDER_UNKNOWN, error.to_string())
            })?;
            match instance {
                ProviderInstance::OpenAi {
                    api_base,
                    api_key_env,
                    ..
                } => (
                    api_base
                        .clone()
                        .unwrap_or_else(|| DEFAULT_API_BASE.to_string()),
                    Some(api_key_env.clone()),
                ),
                ProviderInstance::OpenAiCompat {
                    base_url,
                    api_key_env,
                    ..
                } => (base_url.clone(), api_key_env.clone()),
                other => {
                    return Err(Failure::new(
                        &catalog::CONFIG_INVALID,
                        format!(
                            "provider `{name}` is kind = {:?}, which does not serve the Decisions API",
                            other.kind()
                        ),
                    )
                    .with_fix(
                        "pass `--provider` an instance of kind \"openai\" or \"openai_compat\", \
                         or leave it off to call OpenAI with OPENAI_API_KEY",
                    ));
                }
            }
        }
    };

    let key_env = args.api_key_env.clone().or(configured_env);
    let Some(key_env) = key_env else {
        // An `openai_compat` instance with no key variable: a local or
        // trusted gateway that wants no credential.
        return Ok(Endpoint {
            api_base,
            api_key: String::new(),
            key_env: "(none)".into(),
        });
    };
    let api_key = std::env::var(&key_env).unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(Failure::new(
            &catalog::PROVIDER_FAILED,
            format!("no API key: `{key_env}` is not set"),
        )
        .with_fix(format!(
            "export {key_env}=<key>, or put `{key_env}=<key>` in secrets.env in the config \
             directory; `--api-key-env <VAR>` reads a different variable{}",
            if args.provider.is_none() && args.api_key_env.is_none() {
                " (with neither flag, OPEN_AI is also tried)"
            } else {
                ""
            }
        ))
        .with_detail("api_key_env", &key_env)
        .retryable(false));
    }
    Ok(Endpoint {
        api_base,
        api_key,
        key_env,
    })
}

/// The service's refusal, as an envelope failure whose `fix` says what to do.
fn failure_of(error: &DecisionError, key_env: &str) -> Failure {
    let code = if error.status == Some(429) {
        &catalog::PROVIDER_RATE_LIMITED
    } else {
        &catalog::PROVIDER_FAILED
    };
    let fix = match error.status {
        Some(401 | 403) => format!(
            "the service refused the key in `{key_env}`: check it is a live key for a project \
             with Decisions access, or name another with `--api-key-env <VAR>`"
        ),
        Some(404) => "the model or route is not there: `--model gpt-6-luna` is the Decisions \
                      model, and a `--provider` must serve `/decisions`"
            .to_string(),
        Some(400 | 422) => match &error.param {
            Some(param) => format!("the service rejected `{param}`: correct it and run again"),
            None => "the service rejected the request: correct what the message names and run \
                     again"
                .to_string(),
        },
        Some(429) => "wait and run again; the service is rate limiting this key".to_string(),
        _ => "run again; if it persists, check the endpoint with `--provider` or the network"
            .to_string(),
    };

    let mut failure = Failure::new(code, error.to_string())
        .with_fix(fix)
        .retryable(error.retryable);
    if let Some(status) = error.status {
        failure = failure.with_detail("status", status);
    }
    if let Some(param) = &error.param {
        failure = failure.with_detail("param", param);
    }
    if let Some(wait) = error.retry_after {
        failure = failure.with_detail("retry_after_seconds", wait.as_secs());
    }
    failure
}

fn next_action(answers: &[Answer]) -> String {
    if answers
        .iter()
        .any(|answer| matches!(answer, Answer::Refusal { .. }))
    {
        return "a `refusal` answer means the model declined that question — treat it as \
                unknown, or reword it; the other answers stand"
            .into();
    }
    "answers are in question order: branch on `type`, then read `probability` (predicate), \
     `choice` or `score` — a `confidence` under ~0.6 is worth a second look"
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(prompt: &str) -> DecisionArgs {
        DecisionArgs {
            prompt: Some(prompt.into()),
            model: "gpt-6-luna".into(),
            ..DecisionArgs::default()
        }
    }

    #[test]
    fn a_bare_prompt_is_a_predicate_judged_on_its_own() {
        let args = args("Is Rust memory safe?");
        let mut stdin = Stdin::default();
        let questions = build_questions(&args, &mut stdin).unwrap();
        assert_eq!(
            questions,
            vec![Question::Predicate {
                name: None,
                instructions: "Is Rust memory safe?".into()
            }]
        );
        let (input, summary) = build_input(&args, &mut stdin).unwrap();
        assert_eq!(input, DecisionInput::Text("Is Rust memory safe?".into()));
        assert_eq!(summary.texts, 1);
    }

    #[test]
    fn choices_make_a_choice_question_and_split_at_the_first_equals() {
        let mut args = args("Which team?");
        args.name = Some("team".into());
        args.choices = vec!["billing=charges = refunds".into(), "other".into()];
        let questions = build_questions(&args, &mut Stdin::default()).unwrap();
        let Question::Choice { name, choices, .. } = &questions[0] else {
            panic!("expected a choice: {questions:?}");
        };
        assert_eq!(name.as_deref(), Some("team"));
        assert_eq!(choices[0].value, ChoiceValue::Text("billing".into()));
        assert_eq!(choices[0].description.as_deref(), Some("charges = refunds"));
        assert_eq!(choices[1].description, None);
    }

    #[test]
    fn levels_make_a_score_question() {
        let mut args = args("How severe?");
        args.levels = vec!["low".into(), "high=data loss".into()];
        let questions = build_questions(&args, &mut Stdin::default()).unwrap();
        assert!(matches!(&questions[0], Question::Score { levels, .. } if levels.len() == 2));
    }

    #[test]
    fn a_questions_file_adds_one_or_many_in_the_api_shape() {
        let dir = tempfile::tempdir().unwrap();
        let many = dir.path().join("many.json");
        std::fs::write(
            &many,
            r#"[{"type":"predicate","name":"a","instructions":"A?"},
                {"type":"choice","instructions":"B?","choices":[{"value":true},{"value":false}]}]"#,
        )
        .unwrap();
        let mut args = args("First?");
        args.questions = Some(many.display().to_string());
        let questions = build_questions(&args, &mut Stdin::default()).unwrap();
        assert_eq!(questions.len(), 3);
        assert!(matches!(&questions[2], Question::Choice { choices, .. }
            if choices[0].value == ChoiceValue::Bool(true)));

        let one = dir.path().join("one.json");
        std::fs::write(&one, r#"{"type":"predicate","instructions":"A?"}"#).unwrap();
        let mut args = DecisionArgs {
            questions: Some(one.display().to_string()),
            ..DecisionArgs::default()
        };
        args.inputs = vec!["evidence".into()];
        assert_eq!(
            build_questions(&args, &mut Stdin::default()).unwrap().len(),
            1
        );
    }

    #[test]
    fn a_malformed_questions_file_is_a_usage_failure_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, r#"[{"type":"guess","instructions":"?"}]"#).unwrap();
        let args = DecisionArgs {
            questions: Some(bad.display().to_string()),
            ..DecisionArgs::default()
        };
        let failure = build_questions(&args, &mut Stdin::default()).unwrap_err();
        assert_eq!(failure.code, "FS3-E-USAGE-INVALID");
        assert!(failure.message.contains("bad.json"), "{}", failure.message);
    }

    #[test]
    fn attachments_become_image_and_text_parts_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("photo.PNG");
        std::fs::write(&image, b"\x89PNG").unwrap();
        let notes = dir.path().join("notes.txt");
        std::fs::write(&notes, "the mug is cracked").unwrap();

        let mut args = args("Is it damaged?");
        args.inputs = vec!["order 42".into()];
        args.attachments = vec![
            image.display().to_string(),
            notes.display().to_string(),
            "https://example.com/a.jpg".into(),
        ];
        args.detail = Some(DetailArg::High);

        let (input, summary) = build_input(&args, &mut Stdin::default()).unwrap();
        let DecisionInput::Messages(messages) = input else {
            panic!("several parts are a message");
        };
        let parts = &messages[0].content;
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], InputPart::text("order 42"));
        assert!(
            matches!(&parts[1], InputPart::InputImage { image_url, detail: Some(ImageDetail::High) }
            if image_url.starts_with("data:image/png;base64,"))
        );
        assert!(
            matches!(&parts[2], InputPart::InputText { text } if text.contains("the mug is cracked"))
        );
        assert!(
            matches!(&parts[3], InputPart::InputImage { image_url, .. } if image_url == "https://example.com/a.jpg")
        );
        assert_eq!((summary.texts, summary.images), (2, 2));
    }

    #[test]
    fn a_binary_non_image_attachment_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("blob.bin");
        std::fs::write(&blob, [0xff, 0xfe, 0x00]).unwrap();
        let mut args = args("?");
        args.attachments = vec![blob.display().to_string()];
        let failure = build_input(&args, &mut Stdin::default()).unwrap_err();
        assert!(
            failure.message.contains("neither an image"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn stdin_cannot_be_claimed_twice() {
        let mut stdin = Stdin {
            claimed_by: Some("--questions"),
        };
        let failure = stdin.read("--input").unwrap_err();
        assert!(
            failure.message.contains("already reads stdin"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_missing_key_names_the_variable_and_is_not_retryable() {
        let mut args = args("?");
        args.api_key_env = Some("FS3_TEST_SURELY_UNSET_DECISIONS_KEY".into());
        let dir = tempfile::tempdir().unwrap();
        let effective = crate::settings::load_effective_from(dir.path());
        let Err(failure) = resolve_endpoint(&args, effective.as_ref()) else {
            panic!("an unset variable cannot authenticate");
        };
        assert_eq!(failure.code, "FS3-E-PROVIDER-FAILED");
        assert!(failure.fix.contains("FS3_TEST_SURELY_UNSET_DECISIONS_KEY"));
        assert!(!failure.retryable);
    }

    #[test]
    fn open_ai_is_the_fallback_key_variable() {
        let only =
            |set: &'static str| move |name: &str| (name == set).then(|| "sk-test".to_string());
        assert_eq!(default_key_env(only("OPENAI_API_KEY")), "OPENAI_API_KEY");
        assert_eq!(default_key_env(only("OPEN_AI")), "OPEN_AI");
        assert_eq!(
            default_key_env(|_| Some("sk-test".into())),
            "OPENAI_API_KEY"
        );
        assert_eq!(default_key_env(|_| Some("  ".into())), "OPENAI_API_KEY");
        assert_eq!(default_key_env(|_| None), "OPENAI_API_KEY");
    }

    #[test]
    fn a_rejection_is_explained_with_the_field_the_service_blamed() {
        let failure = failure_of(
            &DecisionError {
                status: Some(400),
                message: "Question names must be unique within the request.".into(),
                param: Some("questions[1].name".into()),
                retryable: false,
                retry_after: None,
            },
            "OPENAI_API_KEY",
        );
        assert_eq!(failure.code, "FS3-E-PROVIDER-FAILED");
        assert!(failure.fix.contains("questions[1].name"));
        assert!(!failure.retryable);
        assert_eq!(failure.details["status"], 400);
    }
}
