# decision

`flowspace3 decision` asks OpenAI's Decisions API one bounded question about
text or images and returns a typed answer: a probability, one of your options,
or a score on your rubric. Nothing is generated, so there is no prose to parse,
and output tokens are never billed. It is a direct call: no daemon, no database.

## Output shape

A TTY (terminal) receives the human reading view. A pipe, file, CI capture, or agent
subprocess receives the JSON envelope with no flag. `--json` forces JSON
anywhere. A harness inside a PTY such as tmux should export `FS3_OUTPUT=json`
because the terminal probe otherwise looks human.

## The three kinds of question

The prompt is the question. What follows it decides the kind:

```bash
# predicate: the probability that the statement is true
flowspace3 decision "Is this a complaint?" --input "I was charged twice."

# choice: one of your options (2-255), each optionally VALUE=DESCRIPTION
flowspace3 decision "Which team handles this?" --input "I was charged twice." \
  --choice "billing=charges, refunds, invoices" --choice shipping --choice other

# score: your levels (2-10), lowest first; the score can fall between levels
flowspace3 decision "How severe is this bug?" --input "Search returns nothing since the deploy." \
  --level cosmetic --level minor --level major --level "critical=data loss or outage"
```

With no `--input` or `--attach`, the prompt is judged on its own:
`flowspace3 decision "Is Rust memory safe?"`.

`--name <name>` labels the prompt's question; the answer echoes it.

## Evidence: text, files, images

- `--input <text>` adds text (repeatable). `--input -` reads stdin.
- `--attach <path>` adds a file: `png`, `jpg`, `gif` and `webp` are sent as
  inline images; any other UTF-8 file is sent as text with its path.
- `--attach https://…` adds an image the service fetches itself. That adds the
  download to the latency (about 1.5 s against about 0.1 s inline), and a host that
  refuses the fetch is a 400 naming `input[0].content[N].image_url`; attach a
  local copy instead.
- `--detail low|high|auto|original` sets how closely images are examined.

Everything is sent in the order given, as one user message.

```bash
flowspace3 decision "Is there visible damage?" --attach ./photo.jpg --detail high
git diff | flowspace3 decision "Does this change touch error handling?" --input -
```

## Many questions at once

`--questions <file>` adds questions written in the API's own shape, one object
or an array. Every question shares the same evidence, and answers come back in
question order. Booleans are valid choice values here.

```json
[
  {"type": "predicate", "name": "angry", "instructions": "Is the customer angry?"},
  {"type": "choice", "name": "dept", "instructions": "Which department?",
   "choices": [{"value": "billing"}, {"value": "technical"}, {"value": "other"}]},
  {"type": "score", "name": "churn", "instructions": "How likely are they to leave?",
   "levels": [{"label": "low"}, {"label": "medium"}, {"label": "high"}]}
]
```

```bash
flowspace3 decision --questions triage.json --input "Charged three times. Fix it or I'm leaving."
```

A question whose wording depends on another's answer needs a second call.

## Reading the answer

```json
{ "ok": true, "command": "decision", "v": 1,
  "data": {
    "model": "gpt-6-luna",
    "questions": [ … as sent … ],
    "answers": [
      {"type": "predicate", "name": "angry", "probability": 0.99},
      {"type": "choice", "name": "dept", "choice": "billing", "confidence": 0.95,
       "probabilities": [{"value": "billing", "probability": 0.96}, …]},
      {"type": "score", "name": "churn", "score": 2.0, "confidence": 1.0,
       "probabilities": [{"value": 0, "label": "low", "probability": 0.0}, …]},
      {"type": "refusal", "name": "…"}
    ],
    "usage": {"input_tokens": 443, "output_tokens": 0, "total_tokens": 443},
    "input": {"texts": 1, "images": 0},
    "endpoint": "https://api.openai.com/v1",
    "latency_ms": 112 } }
```

- Branch on each answer's `type`. `answers[i]` answers `questions[i]`.
- `score` is the probability-weighted mean of 0-based level indices.
- `confidence` under about 0.6 deserves a second look.
- `refusal` means the model declined that one question; the others stand.

## Keys and endpoints

The key is read from `OPENAI_API_KEY` (the environment, or `secrets.env` in
the config directory). `--api-key-env <VAR>` reads another variable.
`--provider <name>` borrows a configured `openai` or `openai_compat` instance's
base URL and key variable — a gateway that serves `/decisions` works the same
way. `--model` defaults to `gpt-6-luna`, the only Decisions model at public beta.
`--safety-identifier <id>` passes a stable pseudonymous end-user id.

A refused request comes back as `FS3-E-PROVIDER-FAILED` with the service's own
sentence, the field it blamed in `error.details.param`, and a `fix`. A busy
service is retried, then reported as `FS3-E-PROVIDER-RATE-LIMITED`.
