//! The real Decisions endpoint answers the shapes the client speaks.
//!
//! Ignored by default because it costs money and needs a key:
//!
//! ```bash
//! OPENAI_API_KEY=sk-… cargo test -p fs3-providers --test decisions_contract -- --ignored
//! ```

use fs3_providers::{
    Answer, ChoiceOption, ChoiceValue, DECISIONS_DEFAULT_MODEL, DecisionInput, DecisionRequest,
    DecisionsClient, InputMessage, InputPart, Question, ScoreLevel,
};

fn api_key() -> String {
    std::env::var("OPENAI_API_KEY").expect(
        "this test is #[ignore]d precisely because it needs a real key; \
         run it with OPENAI_API_KEY set",
    )
}

/// An 8x8 red PNG, generated rather than shipped: no fixture file to vet.
const RED_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x08, 0x02, 0x00, 0x00, 0x00, 0x4b, 0x6d, 0x29,
    0xdc, 0x00, 0x00, 0x00, 0x11, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xb8, 0x23, 0x22, 0x82,
    0x15, 0x31, 0x0c, 0x2d, 0x09, 0x00, 0x98, 0x3f, 0x41, 0x01, 0x9c, 0xc5, 0x8c, 0xb6, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[tokio::test]
#[ignore = "makes real OpenAI API calls; run on demand with OPENAI_API_KEY set"]
async fn every_question_kind_is_answered_in_order() {
    let decision = DecisionsClient::new(None, api_key())
        .create(&DecisionRequest {
            model: DECISIONS_DEFAULT_MODEL.into(),
            input: DecisionInput::Text("I was charged twice for my order.".into()),
            questions: vec![
                Question::Predicate {
                    name: Some("complaint".into()),
                    instructions: "Is this a complaint?".into(),
                },
                Question::Choice {
                    name: Some("dept".into()),
                    instructions: "Which department handles it?".into(),
                    choices: ["billing", "shipping", "other"]
                        .map(|value| ChoiceOption {
                            value: ChoiceValue::Text(value.into()),
                            description: None,
                        })
                        .to_vec(),
                },
                Question::Score {
                    name: Some("severity".into()),
                    instructions: "How severe is it?".into(),
                    levels: ["low", "medium", "high"]
                        .map(|label| ScoreLevel {
                            label: label.into(),
                            description: None,
                        })
                        .to_vec(),
                },
            ],
            safety_identifier: None,
        })
        .await
        .expect("the live endpoint answers");

    let names: Vec<_> = decision.answers.iter().map(Answer::name).collect();
    assert_eq!(names, [Some("complaint"), Some("dept"), Some("severity")]);
    assert!(
        matches!(decision.answers[0], Answer::Predicate { probability, .. } if probability > 0.5)
    );
    assert!(matches!(
        &decision.answers[1],
        Answer::Choice { choice: ChoiceValue::Text(choice), .. } if choice == "billing"
    ));
}

#[tokio::test]
#[ignore = "makes real OpenAI API calls; run on demand with OPENAI_API_KEY set"]
async fn an_inline_image_is_seen() {
    let decision = DecisionsClient::new(None, api_key())
        .create(&DecisionRequest {
            model: DECISIONS_DEFAULT_MODEL.into(),
            input: DecisionInput::Messages(vec![InputMessage {
                content: vec![InputPart::image_bytes("image/png", RED_PNG, None)],
            }]),
            questions: vec![Question::Predicate {
                name: Some("red".into()),
                instructions: "Is this image red?".into(),
            }],
            safety_identifier: None,
        })
        .await
        .expect("the live endpoint answers");

    assert!(
        matches!(decision.answers[0], Answer::Predicate { probability, .. } if probability > 0.5)
    );
}
