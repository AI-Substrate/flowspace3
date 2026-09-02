# flowspace3 dogfood — findings batch 2 from pij-forward-worm — 2026-09-02 — the ask surface
Queue was at 0 open (34 failed, all the known row-133 set) for every probe below.

## F7 — ask over an unparsed language burns its whole iteration budget on one file  (bug, mechanism found)
Question: "How does the cloud preset library decide baseline vs numbered variant, and what if two
files declare the same ordinal?" — answer lives entirely in CloudPresetLibrary.cs (ParseFileName
at :206, duplicate-ordinal throw at :175).
    ask --path 'godot/**/*.cs'  -> FS3-E-QUERY-ASK-ITERATION-LIMIT, 8 iters, 44,566 tokens, 115s, no answer
    ask (unscoped)              -> same limit, 8 iters, 44,852 tokens, 154s, no answer
    ask "what file names does ParseFileName accept?" --path 'godot/**/*.cs'
                                -> answered, grounded=true, 6 iters, 27,393 tokens, 100s
The grounded answer said WHY the others failed, in its own words: "the index returns
CloudPresetLibrary.cs as a single file element that truncates at 7,000 characters, and the
method sits in the truncated tail." Checked: file is 8,058 bytes; 7,000 chars ends at line 205;
ParseFileName is declared at line 206. get on the file element: raw_text len / contains
ParseFileName / children = 8058 True []
`get` returns the full 8,058 chars with the method present, so the STORE is whole; the 7,000-char cut is
in the tool view the ask agent receives. The agent found the right file in iteration 1 both times,
then re-fetched the same truncated view for seven more iterations. It reconstructed
the narrow answer from the unit tests instead, and SAID so — that honesty is the good half.
Asks: (a) surface the truncation in the tool result the agent sees ("7,000 of 8,058 chars; tail
not available") so it stops re-fetching; (b) let the agent request a byte/line window of a file
element, which would make unparsed languages usable at all; (c) this is the concrete cost of
row 136(b) — for ask, every unparsed-language file over 7k is unreadable past the fold, even
though get can read it.

## F8 — the partial-evidence envelope is good  (good)
error.details.evidence carried the 4 citations it had and an iteration ledger; next_action
named the config knob. I could see it had the right file without re-running anything.

## F9 — ask --conversation <my guid>  (good, verified against the source)
"What did Jordan decide about presets/oktas/heights and what was ruled out?" -> grounded,
5 turn citations, 117s. I checked the answer against the brief I wrote (docs/plans/013-cloud-
presets/coder-brief.md): three-dials model correct, four out-of-scope items correct
(second deck, tuned library, live GAF, in-app editor). No hallucinated ruling. Best result
of the day.

## F10 — my own misread, retracted before filing
I first read the F7 envelope as malformed JSON ("Extra data at char 2100"). Cause: I had
merged stderr into the capture, and the human error line rides stderr after the JSON
envelope. Envelope is well-formed; parser was mine. Filed only so the next reader doesn't
repeat it: capture stdout alone when you want the envelope.

## Numbers (loaded box, queue empty)
ask scoped/unscoped/narrow/conversation: 115s / 154s / 100s / 117s.
