// {{MARKER}} (flowspace3 {{VERSION}}); edits are overwritten by the next install.
// A prompt with an `fs3 "<search>"` in it runs a flowspace3 search before the
// agent's turn, and the hits reach the agent as hidden context.
// Guide: `flowspace3 docs get prompt-search`. Remove: `flowspace3 hooks uninstall --harness copilot`.
import { execFile } from "node:child_process";
import { joinSession } from "@github/copilot-sdk/extension";

const FS3 = {{FS3}};
const TIMEOUT_MS = 28000; // the hook bounds its own search at 25s

function search(input) {
	return new Promise((resolve) => {
		const child = execFile(
			FS3,
			["hooks", "prompt", "--harness", "{{HARNESS}}"],
			{ timeout: TIMEOUT_MS, maxBuffer: 1 << 20 },
			(error, stdout) => resolve(error ? "" : String(stdout).trim()),
		);
		child.stdin.end(
			JSON.stringify({
				prompt: input.prompt,
				session_id: input.sessionId ?? "",
				cwd: input.workingDirectory ?? process.cwd(),
			}),
		);
	});
}

await joinSession({
	hooks: {
		onUserPromptSubmitted: async (input) => {
			// Cheap pre-check; the binary owns the grammar.
			if (!/fs3/i.test(input?.prompt ?? "")) return;
			const context = await search(input);
			return context ? { additionalContext: context } : undefined;
		},
	},
});
