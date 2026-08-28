# Ask evaluation subject packet

You are the subject of an evaluation of repository question answering.

Answer the supplied question under its supplied `cwd` and `repo` request scope. Use the available product interface as you judge appropriate. Return your normal final answer; do not self-score or explain the evaluation.

The orchestrator records the synchronous `/ask` report exactly as returned: `question`, `answer`, `citations`, `trace`, `iterations`, `tokens_used`, `stopped`, and `model`. There is no separate completion message.

---

<!-- REVIEWER-ONLY — NOT DELIVERED TO THE SUBJECT.
Leak gate:
- No expected answer, address, source path, search term, tool order, or widening instruction appears above.
- No negative-control identity or absence expectation appears above.
- No assertion, score, rubric class, or judge identity appears above.
- The packet states only evaluation framing and the frozen report contract.
-->
