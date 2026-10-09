// {{MARKER}} (flowspace3 {{VERSION}}); edits are overwritten by the next install.
// A prompt with an `fs3 "<search>"` in it runs a flowspace3 search before the
// agent's turn, and the hits reach the agent as a hidden session message.
// Guide: `flowspace3 docs get prompt-search`. Remove: `flowspace3 hooks uninstall --harness {{HARNESS}}`.

const FS3 = {{FS3}};
const TIMEOUT_MS = 28000; // the hook bounds its own search at 25s; {{NAME}} drops a handler at 30s

export default function (pi: any) {
	pi.on("before_agent_start", async (event: any, ctx: any) => {
		const prompt: string = event?.prompt ?? "";
		// Cheap pre-check; the binary owns the grammar.
		if (!/fs3/i.test(prompt)) return;
		const session: string = ctx?.sessionManager?.getSessionId?.() ?? "";
		const cwd: string = ctx?.cwd ?? process.cwd();
		const result = await pi.exec(
			FS3,
			["hooks", "prompt", "--harness", "{{HARNESS}}", `--prompt=${prompt}`, `--session=${session}`, `--cwd=${cwd}`],
			{ timeout: TIMEOUT_MS },
		);
		const context = String(result?.stdout ?? "").trim();
		if (result?.code !== 0 || !context) return;
		return { message: { customType: "fs3-search", content: context, display: false } };
	});
}
