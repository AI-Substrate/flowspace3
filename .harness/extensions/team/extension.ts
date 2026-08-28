import { createHash } from 'node:crypto';
import { basename, join, sep } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';
import type { V2VerbContext, VerbResult } from '@ai-substrate/engineering-harness/contract';

/**
 * `harness team new <slug>` — cut a worktree + branch for a pij-team plan and
 * scaffold the next-ordinal plan folder inside it.
 *
 * Ordinals are repo-wide and must never be handed out twice, so the scan reads
 * every source an ordinal can hide in: this clone, every registered worktree,
 * and every local branch head (a plan folder committed on a branch nobody has
 * checked out is invisible to a filesystem walk — that was the race the POC
 * left open).
 *
 * Ported from scratch/team-new-poc/team-new.mjs; see
 * scratch/team-new-poc/validation.md for the POC transcripts.
 *
 * `harness team tidy <slug>` — the teardown counterpart. It exists because the
 * absence of it was already costing us: `worktreePaths` below stats every path
 * precisely because a hand-tidy leaves the registry naming trees that are gone,
 * and `E_BRANCH_EXISTS` on the mint side names "a tidy that removed the worktree
 * but not the branch" as its usual cause. Both are scars from this missing verb.
 */

/** Seeded beside the plan so prime can fill them in place. */
const TEMPLATES = [
  'impl-guide.dd.json',
  'packet-pm.dd.json',
  'packet-coder.dd.json',
  'packet-reviewer.dd.json',
];
const TEMPLATE_DIR = '.agents/skills/pij-team/templates';
const PLANS_DIR = 'docs/plans';

/** A plan folder is `NNN-slug`; anything else under docs/plans (prd/, conversations/) is not one. */
const ORDINAL = /^(\d+)-/;
const KEBAB = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

const GIT_TIMEOUT = 60_000;
const SCAFFOLD_TIMEOUT = 120_000;

type Source = 'worktree' | 'branch';
type Found = { ordinal: number; path: string; source: Source };

/** Every git call in this verb is repo-local and short; the timeout is not per-call taste. */
async function git(ctx: V2VerbContext, args: string[], cwd: string) {
  return ctx.exec('git', args, { cwd, timeoutMs: GIT_TIMEOUT });
}

/**
 * Registered worktrees that actually exist. `git worktree list` is a registry,
 * not a filesystem oracle — it names paths that have already been removed
 * (observed 2026-08-28, mid-tidy), so every path is stat-ed before it is read.
 */
async function worktreePaths(ctx: V2VerbContext, root: string): Promise<string[]> {
  const listed = await git(ctx, ['worktree', 'list', '--porcelain'], root);
  const paths = [root];
  if (listed.ok) {
    for (const line of listed.stdout.split('\n')) {
      if (line.startsWith('worktree ')) paths.push(line.slice('worktree '.length).trim());
    }
  }
  return [...new Set(paths)].filter((p) => ctx.fs.exists(p));
}

/** Ordinals on disk under one `docs/plans`, descending one level into `archive*`. */
function ordinalsOnDisk(ctx: V2VerbContext, dir: string, depth = 1): Found[] {
  if (!ctx.fs.exists(dir)) return [];
  const found: Found[] = [];
  for (const name of ctx.fs.readdir(dir)) {
    const match = ORDINAL.exec(name);
    if (match) found.push({ ordinal: Number(match[1]), path: join(dir, name), source: 'worktree' });
    else if (depth > 0 && name.startsWith('archive')) found.push(...ordinalsOnDisk(ctx, join(dir, name), depth - 1));
  }
  return found;
}

/** Directory entries a ref holds under `path` (trailing slash lists contents). */
async function treeDirs(ctx: V2VerbContext, root: string, ref: string, path: string): Promise<string[]> {
  const tree = await git(ctx, ['ls-tree', '-d', '--name-only', ref, path], root);
  return tree.ok ? tree.stdout.split('\n').map((l) => l.trim()).filter(Boolean) : [];
}

/**
 * Ordinals committed on local branch heads. This is the half a filesystem walk
 * cannot see: a plan folder on a branch with no worktree is real, claimed, and
 * invisible — two seats would otherwise mint the same ordinal days apart.
 */
async function ordinalsOnBranches(ctx: V2VerbContext, root: string): Promise<{ found: Found[]; branches: number }> {
  const refs = await git(ctx, ['for-each-ref', '--format=%(refname:short)', 'refs/heads'], root);
  const branches = refs.ok ? refs.stdout.split('\n').map((l) => l.trim()).filter(Boolean) : [];
  const found: Found[] = [];
  for (const ref of branches) {
    for (const entry of await treeDirs(ctx, root, ref, `${PLANS_DIR}/`)) {
      const name = basename(entry);
      const match = ORDINAL.exec(name);
      if (match) {
        found.push({ ordinal: Number(match[1]), path: `${ref}:${entry}`, source: 'branch' });
        continue;
      }
      if (!name.startsWith('archive')) continue;
      for (const archived of await treeDirs(ctx, root, ref, `${entry}/`)) {
        const inner = ORDINAL.exec(basename(archived));
        if (inner) found.push({ ordinal: Number(inner[1]), path: `${ref}:${archived}`, source: 'branch' });
      }
    }
  }
  return { found, branches: branches.length };
}

/**
 * The main clone. A linked worktree's git dir lives under `…/worktrees/`; the
 * main clone's does not. Worth refusing over: run from a worktree and
 * `../fs3-<slug>` lands beside another seat's tree instead of beside the clone.
 */
async function mainClone(ctx: V2VerbContext): Promise<{ root: string } | { refusal: VerbResult }> {
  if (!ctx.git.isRepo()) {
    return {
      refusal: ctx.error('E_NOT_A_REPO', 'not inside a git repository', {
        next_action: 'cd into the main clone and re-run `harness team new <slug>`.',
      }),
    };
  }
  const top = await git(ctx, ['rev-parse', '--show-toplevel'], ctx.cwd);
  const root = top.stdout.trim();
  const gitDir = await git(ctx, ['rev-parse', '--absolute-git-dir'], root);
  if (gitDir.stdout.includes(`${sep}worktrees${sep}`)) {
    return {
      refusal: ctx.error('E_LINKED_WORKTREE', `this is a linked worktree, not the main clone: ${root}`, {
        next_action: 'cd to the main clone and re-run — worktrees are created beside it, never nested inside another one.',
      }),
    };
  }
  return { root };
}

async function runNew(ctx: V2VerbContext): Promise<VerbResult> {
  const slug = typeof ctx.args.slug === 'string' ? ctx.args.slug.trim() : '';
  const propose = Boolean(ctx.options.propose);

  if (!KEBAB.test(slug)) {
    return ctx.error('E_BAD_SLUG', `slug "${slug}" is not kebab-case ([a-z0-9] words joined by single hyphens)`, {
      next_action: 'Re-run with a kebab-case slug, e.g. `harness team new conversation-ingest`.',
    });
  }

  const located = await mainClone(ctx);
  if ('refusal' in located) return located.refusal;
  const root = located.root;

  // --- scan every source an ordinal can hide in ---------------------------
  const worktrees = await worktreePaths(ctx, root);
  const found: Found[] = [];
  for (const worktree of worktrees) found.push(...ordinalsOnDisk(ctx, join(worktree, PLANS_DIR)));
  const branchScan = await ordinalsOnBranches(ctx, root);
  found.push(...branchScan.found);

  if (found.length === 0) {
    return ctx.error('E_NO_ORDINALS', `no NNN- plan folders found under ${PLANS_DIR} in any worktree or on any local branch`, {
      next_action: 'Confirm you are in the right repository. The FIRST plan ordinal is created by hand, deliberately — this verb only ever continues a sequence.',
    });
  }

  const highest = found.reduce((a, b) => (b.ordinal > a.ordinal ? b : a));
  const ordinal = String(highest.ordinal + 1).padStart(3, '0');
  const branch = `${ordinal}-${slug}`;
  const worktree = join(root, '..', `fs3-${slug}`);
  const planFolder = join(worktree, PLANS_DIR, `${ordinal}-${slug}`);

  const planned = {
    worktree,
    branch,
    ordinal,
    plan_folder: planFolder,
    documents: [
      join(planFolder, 'plan.dd.json'),
      join(planFolder, 'assets', 'tasks', 'phase-1', 'tasks.dd.json'),
      ...TEMPLATES.map((name) => join(planFolder, name)),
    ],
    rendered: [
      join(planFolder, 'plan.dd.md'),
      join(planFolder, 'assets', 'tasks', 'phase-1', 'tasks.dd.md'),
      ...TEMPLATES.map((name) => join(planFolder, name.replace(/\.json$/, '.md'))),
    ],
  };
  const scan = {
    scanned_worktrees: worktrees.length,
    scanned_branches: branchScan.branches,
    highest_existing: { ordinal: String(highest.ordinal).padStart(3, '0'), path: highest.path, source: highest.source },
  };

  // --- propose: answer the question, mint nothing -------------------------
  // A read-only query that succeeded is `ok`; the kernel's status vocabulary is
  // ok|degraded|unconfigured|error and is not ours to widen, so the proposal
  // says so in `mode`. Nothing here is reserved: the mint re-scans.
  if (propose) {
    return ctx.ok(
      {
        mode: 'propose',
        reserved: false,
        advisory: `ordinal ${ordinal} is advisory, not reserved — whichever seat mints first takes it, and \`harness team new ${slug}\` re-scans before it creates anything`,
        ...scan,
        would_create: planned,
      },
      {
        evidence: [{ label: 'proposal', none: true }],
        next_action: `Nothing was created. Run \`harness team new ${slug}\` to mint it.`,
      },
    );
  }

  // --- refusals, before anything is mutated -------------------------------
  if (ctx.fs.exists(worktree)) {
    return ctx.error('E_WORKTREE_EXISTS', `worktree path already exists: ${worktree}`, {
      next_action: `Pick another slug, or remove the existing worktree with \`git worktree remove ${worktree}\`.`,
    });
  }
  const branchRef = await git(ctx, ['show-ref', '--verify', '--quiet', `refs/heads/${branch}`], root);
  if (branchRef.ok) {
    return ctx.error('E_BRANCH_EXISTS', `branch already exists: ${branch}`, {
      next_action: `Delete it with \`git branch -D ${branch}\`, or pick another slug.`,
    });
  }
  const fsWrite = ctx.fsWrite;
  if (!fsWrite) {
    return ctx.error('E_CORE_TOO_OLD', 'this harness core provides no write-side filesystem capability, so the templates cannot be seeded', {
      next_action: 'Upgrade the harness CLI (`npm i -g @ai-substrate/engineering-harness`). `harness team new <slug> --propose` still works — it writes nothing.',
    });
  }
  const templateDir = join(root, TEMPLATE_DIR);
  if (!ctx.fs.exists(templateDir)) {
    return ctx.error('E_TEMPLATES_MISSING', `pij-team templates not found: ${templateDir}`, {
      next_action: 'The pij-team skill is tracked in this repo — restore `.agents/skills/pij-team/templates/`, then re-run.',
    });
  }

  // --- mint ---------------------------------------------------------------
  // Past this line a failure can leave a half-built worktree, which is worse
  // than none: unwind it and say so.
  let created = false;
  try {
    const add = await git(ctx, ['worktree', 'add', worktree, '-b', branch], root);
    if (!add.ok) throw new Error(`git worktree add failed: ${(add.stderr || add.stdout).trim()}`);
    created = true;

    const scaffold = await ctx.exec('harness', ['plan', 'new', slug, '--ordinal', ordinal], {
      cwd: worktree,
      timeoutMs: SCAFFOLD_TIMEOUT,
    });
    if (!scaffold.ok) throw new Error(`harness plan new failed: ${(scaffold.stderr || scaffold.stdout).trim()}`);
    const scaffolded = JSON.parse(scaffold.stdout) as {
      status: string;
      data: { folder: string; documents: string[]; rendered: string[] };
    };
    if (scaffolded.status !== 'ok') throw new Error(`harness plan new refused: ${scaffold.stdout.trim()}`);

    const folder = scaffolded.data.folder;
    const seeded: string[] = [];
    for (const name of TEMPLATES) {
      // `confineRoot` confines the SOURCE: the bytes must come from inside the
      // skill's template folder, resolved, so a swapped symlink cannot redirect
      // the read into something else on the way past.
      if (!fsWrite.copy(join(templateDir, name), folder, { confineRoot: templateDir })) {
        throw new Error(`could not copy template ${name} from ${templateDir} into ${folder}`);
      }
      seeded.push(join(folder, name));
    }

    // `ddocs build` resolves schemas from CWD, not from the document's
    // ancestors, so it MUST run with cwd = the worktree root (CONF-009).
    const rendered: string[] = [];
    for (const document of seeded) {
      const relative = document.slice(worktree.length + 1);
      const built = await ctx.exec('ddocs', ['build', relative], { cwd: worktree, timeoutMs: SCAFFOLD_TIMEOUT });
      if (!built.ok) {
        const output = `${built.stdout}${built.stderr}`;
        if (output.includes('E401') || output.includes('was not found in any discovery root')) {
          throw new Error(
            `ddocs could not resolve the pij-team schemas in the new worktree (${relative}). They are tracked in .dd/schemas/pij-team/ — if they are missing here, the branch predates that commit.`,
          );
        }
        throw new Error(`ddocs build failed for ${relative}: ${(built.stderr || built.stdout).trim()}`);
      }
      rendered.push(JSON.parse(built.stdout).data.sibling as string);
    }

    return ctx.ok(
      {
        mode: 'mint',
        ...scan,
        worktree,
        branch,
        ordinal,
        plan_folder: folder,
        documents: [...scaffolded.data.documents, ...seeded],
        rendered: [...scaffolded.data.rendered, ...rendered],
      },
      {
        evidence: [{ label: 'plan folder', path: folder }],
        next_action: 'prime: write the plan, then the impl-guide',
      },
    );
  } catch (error) {
    let rolledBack = false;
    if (created) {
      const removed = await git(ctx, ['worktree', 'remove', '--force', worktree], root);
      const deleted = await git(ctx, ['branch', '-D', branch], root);
      rolledBack = removed.ok && deleted.ok;
    }
    return ctx.error('E_SCAFFOLD_FAILED', error instanceof Error ? error.message : String(error), {
      details: { worktree, branch, ordinal, rolled_back: rolledBack },
      next_action: rolledBack
        ? 'The partial worktree and branch were removed. Fix the cause above, then re-run.'
        : `Fix the cause above. The worktree ${worktree} and branch ${branch} may still exist — remove them with \`git worktree remove --force\` and \`git branch -D\` before re-running.`,
    });
  }
}


// ---------------------------------------------------------------------------
// tidy — the teardown counterpart to `new`
// ---------------------------------------------------------------------------

/** Where a seat's observation buffer lives inside its worktree. */
const BUFFER_REL = join('.harness', 'temp', 'agent', 'session-buffer.md');
/** Where a rescued buffer lands in the main clone — beside the live one, same gitignored area. */
const RESCUE_DIR = join('.harness', 'temp', 'agent');

const sha256 = (text: string) => createHash('sha256').update(text, 'utf8').digest('hex');

type WorktreeRecord = { path: string; branch: string | null };

/** `git worktree list --porcelain` parsed into (path, branch) pairs. */
async function worktreeRecords(ctx: V2VerbContext, root: string): Promise<WorktreeRecord[]> {
  const listed = await git(ctx, ['worktree', 'list', '--porcelain'], root);
  if (!listed.ok) return [];
  const records: WorktreeRecord[] = [];
  let current: WorktreeRecord | null = null;
  for (const line of listed.stdout.split('\n')) {
    if (line.startsWith('worktree ')) {
      if (current) records.push(current);
      current = { path: line.slice('worktree '.length).trim(), branch: null };
    } else if (line.startsWith('branch ') && current) {
      current.branch = line.slice('branch refs/heads/'.length).trim();
    }
  }
  if (current) records.push(current);
  return records;
}

/**
 * The observation buffer is the one thing in a worktree that is NOT
 * regenerable and NOT committed: it is gitignored by construction, so removing
 * the tree destroys it silently. That is DL-027, and it has cost us real
 * observations. So the rescue runs before ANY mutation and before any refusal
 * that would abort — a tidy that refuses must still not lose the buffer — and
 * it is verified by hashing the bytes we wrote and the bytes that came back.
 */
async function rescueBuffer(
  ctx: V2VerbContext,
  root: string,
  worktree: string,
  slug: string,
): Promise<{ rescued: null | { from: string; to: string; sha256: string; bytes: number }; refusal?: VerbResult }> {
  const source = join(worktree, BUFFER_REL);
  if (!ctx.fs.exists(source)) return { rescued: null };
  const text = ctx.fs.readText(source);
  if (text === null || text.trim() === '') return { rescued: null };

  const fsWrite = ctx.fsWrite;
  if (!fsWrite) {
    return {
      rescued: null,
      refusal: ctx.error('E_CORE_TOO_OLD', 'this harness core provides no write-side filesystem capability, so the observation buffer cannot be rescued', {
        details: { buffer: source, bytes: text.length },
        next_action: 'Upgrade the harness CLI (`npm i -g @ai-substrate/engineering-harness`), or copy the buffer out by hand before tidying. Nothing was removed.',
      }),
    };
  }

  // Never overwrite a previous rescue — a second tidy of the same slug, or a
  // re-run after a refusal, must not clobber what the first one saved.
  const dir = join(root, RESCUE_DIR);
  fsWrite.mkdirp(dir);
  let dest = join(dir, `${slug}-observations.md`);
  if (ctx.fs.exists(dest)) {
    dest = join(dir, `${slug}-observations-${ctx.clock.nowIso().replace(/[:.]/g, '-')}.md`);
  }

  const expected = sha256(text);
  fsWrite.writeText(dest, text);
  const readBack = ctx.fs.readText(dest);
  if (readBack === null || sha256(readBack) !== expected) {
    return {
      rescued: null,
      refusal: ctx.error('E_RESCUE_FAILED', `the observation buffer was copied to ${dest} but the copy does not hash-match the source — refusing to remove the worktree`, {
        details: { from: source, to: dest, expected_sha256: expected, actual_sha256: readBack === null ? null : sha256(readBack) },
        next_action: 'Copy the buffer out by hand and verify it, then re-run. NOTHING was removed — the worktree, its branch and its volumes are all still here.',
      }),
    };
  }
  return { rescued: { from: source, to: dest, sha256: expected, bytes: text.length } };
}

/** Uncommitted work, including untracked-but-not-ignored files. */
async function dirtyPaths(ctx: V2VerbContext, worktree: string): Promise<string[]> {
  const status = await git(ctx, ['status', '--porcelain'], worktree);
  return status.ok ? status.stdout.split('\n').map((l) => l.trim()).filter(Boolean) : [];
}

/**
 * Commits that exist only here. No upstream means EVERY commit off the merge
 * base is unpushed — the dangerous case, not the safe one, so it is reported
 * as such rather than skipped.
 */
async function unpushedCommits(ctx: V2VerbContext, worktree: string, root: string, branch: string | null): Promise<string[]> {
  if (!branch) return [];
  const upstream = await git(ctx, ['rev-parse', '--abbrev-ref', `${branch}@{upstream}`], worktree);
  const range = upstream.ok && upstream.stdout.trim() ? `${upstream.stdout.trim()}..${branch}` : null;
  const spec = range ?? `origin/main..${branch}`;
  const log = await git(ctx, ['log', '--oneline', '--no-decorate', spec], root);
  if (!log.ok) return [];
  return log.stdout.split('\n').map((l) => l.trim()).filter(Boolean);
}

/**
 * Is the branch already merged into main?
 *
 * TWO legs, and the second is not optional. The ancestor check alone was the
 * original bug (DL-049): this repo SQUASH-merges, which rewrites history, so a
 * correctly-landed branch's tip is never an ancestor of main and
 * `git branch --merged` never lists it. Every properly merged packet branch hit
 * `E_BRANCH_NOT_MERGED` and had to be tidied with `--force` — which also
 * silences the dirty-tree and unpushed-commit refusals. A safety rail that
 * trains people to `--force` past ALL the checks is worse than the gap it was
 * guarding, so squash detection belongs here rather than in a doc telling
 * people the refusal is usually spurious.
 *
 * Leg 1 — ancestor: cheap, and correct for a real merge commit or fast-forward.
 * Leg 2 — `git cherry <base> <branch>`: one line per commit on the branch,
 *   `-` when an equivalent patch is already upstream, `+` when it is not. All
 *   `-` means the content landed under a different sha, which is exactly what a
 *   squash merge looks like.
 *
 * It fails CLOSED. Anything unproven — a failed command, a `+` line, a base
 * that will not resolve — reports NOT merged, because the cost of guessing
 * "merged" is deleting work that exists nowhere else.
 */
async function isMerged(ctx: V2VerbContext, root: string, branch: string): Promise<boolean> {
  for (const base of ['main', 'origin/main']) {
    const merged = await git(ctx, ['branch', '--merged', base, '--format=%(refname:short)'], root);
    if (!merged.ok) continue;
    if (merged.stdout.split('\n').map((l) => l.trim()).includes(branch)) return true;

    // Leg 2: the squash case. `git cherry` compares patch-ids, so it sees
    // content that landed upstream under a different commit.
    const cherry = await git(ctx, ['cherry', base, branch], root);
    if (!cherry.ok) return false;
    const lines = cherry.stdout.split('\n').map((l) => l.trim()).filter(Boolean);
    // No commits ahead at all — nothing to lose.
    if (lines.length === 0) return true;
    // A single `+` is enough to refuse: a branch that is half-upstream and half
    // novel still holds work that exists nowhere else.
    return lines.every((line) => line.startsWith('-'));
  }
  return false;
}

/**
 * Volumes in this slug's compose namespace only. Compose derives the project
 * name from the directory, so a worktree at `fs3-<slug>` owns `fs3-<slug>_*`
 * and nothing else. Other projects' volumes are named in instructions.md for a
 * human to remove — this verb never touches them, and never prunes globally.
 */
async function slugVolumes(
  ctx: V2VerbContext,
  root: string,
  slug: string,
): Promise<{ available: boolean; removable: string[]; in_use: string[]; detail?: string }> {
  const engine = ctx.env.get('FS3_ENGINE') ?? 'docker';
  const listed = await ctx.exec(engine, ['volume', 'ls', '--format', '{{.Name}}'], { cwd: root, timeoutMs: GIT_TIMEOUT });
  if (!listed.ok) {
    return { available: false, removable: [], in_use: [], detail: (listed.stderr || listed.stdout).trim().split('\n').slice(-1)[0] || `${engine} unreachable` };
  }
  const prefix = `fs3-${slug}_`;
  const mine = listed.stdout.split('\n').map((l) => l.trim()).filter((n) => n.startsWith(prefix));
  const removable: string[] = [];
  const inUse: string[] = [];
  for (const name of mine) {
    // A volume with any container attached — running or not — is in use. This
    // is the zero-link test, done by asking rather than by parsing sizes.
    const users = await ctx.exec(engine, ['ps', '-a', '--filter', `volume=${name}`, '-q'], { cwd: root, timeoutMs: GIT_TIMEOUT });
    if (users.ok && users.stdout.trim() === '') removable.push(name);
    else inUse.push(name);
  }
  return { available: true, removable, in_use: inUse };
}

async function runTidy(ctx: V2VerbContext): Promise<VerbResult> {
  const slug = typeof ctx.args.slug === 'string' ? ctx.args.slug.trim() : '';
  // `--propose` is an alias for `--dry-run`: one preview vocabulary across the
  // verb family, so `new --propose` and `tidy --propose` mean the same thing.
  const dryRun = Boolean(ctx.options.dryRun) || Boolean(ctx.options.propose);
  const force = Boolean(ctx.options.force);
  const alsoRemote = Boolean(ctx.options.remote);

  if (!KEBAB.test(slug)) {
    return ctx.error('E_BAD_SLUG', `slug "${slug}" is not kebab-case ([a-z0-9] words joined by single hyphens)`, {
      next_action: 'Re-run with the slug you passed to `harness team new`, e.g. `harness team tidy conversation-ingest`.',
    });
  }

  const located = await mainClone(ctx);
  if ('refusal' in located) return located.refusal;
  const root = located.root;

  // --- resolve the target -------------------------------------------------
  const worktree = join(root, '..', `fs3-${slug}`);
  const records = await worktreeRecords(ctx, root);
  const registered = records.find((r) => ctx.fs.realpath(r.path) === ctx.fs.realpath(worktree));
  const onDisk = ctx.fs.exists(worktree);

  // The branch: from the registry when the tree is registered, else the
  // `NNN-<slug>` plan branch left behind by a hand-tidy — which is exactly the
  // orphan `E_BRANCH_EXISTS` complains about, and which this verb can clear.
  let branch = registered?.branch ?? null;
  if (!branch) {
    const heads = await git(ctx, ['for-each-ref', '--format=%(refname:short)', 'refs/heads'], root);
    const pattern = new RegExp(`^\\d+-${slug}$`);
    branch = heads.ok ? (heads.stdout.split('\n').map((l) => l.trim()).find((b) => pattern.test(b)) ?? null) : null;
  }

  if (!onDisk && !registered && !branch) {
    return ctx.error('E_NOTHING_TO_TIDY', `no worktree at ${worktree}, no registry entry for it, and no NNN-${slug} branch`, {
      details: { worktree, scanned_worktrees: records.length },
      next_action: 'Nothing to do — this slug is already tidy. Check the slug spelling against `git worktree list`.',
    });
  }

  // --- RESCUE FIRST: before any mutation, before any refusal --------------
  // Ordering is the whole point. A tidy that refuses on a dirty tree must
  // still have saved the buffer, because the next thing the operator does is
  // re-run with --force.
  let rescued: { from: string; to: string; sha256: string; bytes: number } | null = null;
  let wouldRescue: { from: string; bytes: number } | null = null;
  if (onDisk) {
    if (dryRun) {
      const source = join(worktree, BUFFER_REL);
      const text = ctx.fs.exists(source) ? ctx.fs.readText(source) : null;
      if (text !== null && text.trim() !== '') wouldRescue = { from: source, bytes: text.length };
    } else {
      const attempt = await rescueBuffer(ctx, root, worktree, slug);
      if (attempt.refusal) return attempt.refusal;
      rescued = attempt.rescued;
    }
  }

  // --- gather what removal would cost ------------------------------------
  const dirty = onDisk ? await dirtyPaths(ctx, worktree) : [];
  const unpushed = onDisk ? await unpushedCommits(ctx, worktree, root, branch) : [];
  const merged = branch ? await isMerged(ctx, root, branch) : false;
  const volumes = await slugVolumes(ctx, root, slug);

  const wouldRemove = {
    worktree: onDisk ? worktree : null,
    registry_entry: registered ? registered.path : null,
    branch,
    branch_merged: merged,
    remote_branch: alsoRemote && branch ? `origin/${branch}` : null,
    volumes: volumes.removable,
    volumes_in_use_kept: volumes.in_use,
    docker: volumes.available ? 'available' : `unavailable — ${volumes.detail}`,
    would_rescue: wouldRescue,
    at_risk: { uncommitted: dirty, unpushed_commits: unpushed },
  };

  // --- dry run: answer, touch nothing ------------------------------------
  if (dryRun) {
    return ctx.ok(
      { mode: 'dry-run', removed: false, ...wouldRemove },
      {
        evidence: [{ label: 'dry run', none: true }],
        next_action: `Nothing was removed. Run \`harness team tidy ${slug}\` to do it${dirty.length || unpushed.length ? ' — note the at_risk block; it will refuse without --force' : ''}.`,
      },
    );
  }

  // --- refusals, naming exactly what would be lost ------------------------
  // Two distinct conditions, two distinct codes. A clean tree that is merely
  // unpushed is NOT dirty, and a refusal whose code says otherwise sends the
  // operator looking for uncommitted files that do not exist.
  const stillHere = rescued ? `The observation buffer was already rescued to ${rescued.to}.` : 'Nothing was removed.';
  if (!force && dirty.length > 0) {
    return ctx.error('E_WORKTREE_DIRTY', `${worktree} has ${dirty.length} uncommitted change(s) — refusing to remove it`, {
      details: { uncommitted: dirty, unpushed_commits: unpushed, rescued },
      next_action: `Commit them, or re-run with --force to discard them. ${stillHere}`,
    });
  }
  if (!force && unpushed.length > 0) {
    return ctx.error('E_UNPUSHED_COMMITS', `${branch ?? worktree} has ${unpushed.length} commit(s) that exist nowhere else — refusing to remove it`, {
      details: { unpushed_commits: unpushed, rescued },
      next_action: `Push the branch, or re-run with --force to discard those commits. ${stillHere}`,
    });
  }
  if (branch && !merged && !force) {
    return ctx.error('E_BRANCH_NOT_MERGED', `branch ${branch} is not merged into main — refusing to delete it`, {
      details: { branch, unpushed_commits: unpushed, rescued },
      next_action: `Merge the PR first, or re-run with --force to delete it anyway. ${stillHere}`,
    });
  }

  // --- remove -------------------------------------------------------------
  const removed: Record<string, unknown> = { rescued };
  const problems: string[] = [];

  if (onDisk || registered) {
    const args = ['worktree', 'remove', ...(force ? ['--force'] : []), worktree];
    const rm = await git(ctx, args, root);
    if (rm.ok) removed.worktree = worktree;
    else problems.push(`worktree remove: ${(rm.stderr || rm.stdout).trim()}`);
    const pruned = await git(ctx, ['worktree', 'prune'], root);
    removed.registry_pruned = pruned.ok;
  }

  if (branch) {
    const del = await git(ctx, ['branch', force ? '-D' : '-d', branch], root);
    if (del.ok) removed.branch = branch;
    else problems.push(`branch delete: ${(del.stderr || del.stdout).trim()}`);
    if (alsoRemote && del.ok) {
      const pushed = await git(ctx, ['push', 'origin', '--delete', branch], root);
      if (pushed.ok) removed.remote_branch = `origin/${branch}`;
      else problems.push(`remote branch delete: ${(pushed.stderr || pushed.stdout).trim()}`);
    }
  }

  const engine = ctx.env.get('FS3_ENGINE') ?? 'docker';
  const droppedVolumes: string[] = [];
  for (const name of volumes.removable) {
    const drop = await ctx.exec(engine, ['volume', 'rm', name], { cwd: root, timeoutMs: GIT_TIMEOUT });
    if (drop.ok) droppedVolumes.push(name);
    else problems.push(`volume rm ${name}: ${(drop.stderr || drop.stdout).trim()}`);
  }
  removed.volumes = droppedVolumes;
  removed.volumes_in_use_kept = volumes.in_use;

  const payload = {
    mode: 'tidy',
    ...removed,
    docker: volumes.available ? 'available' : `unavailable — ${volumes.detail}`,
    forced: force,
    problems,
  };

  // A partial tidy is reported as partial, never as success. The operator has
  // to know which half happened — that asymmetry is what E_BRANCH_EXISTS on
  // the mint side is made of.
  if (problems.length > 0) {
    return ctx.degraded(
      payload,
      `Tidy was partial — ${problems.length} step(s) failed (see problems). Resolve them by hand; re-running tidy is safe and will finish what is left.`,
      { evidence: rescued ? [{ label: 'rescued observations', path: rescued.to }] : [{ label: 'tidy', none: true }] },
    );
  }
  if (!volumes.available) {
    return ctx.degraded(
      payload,
      `Worktree and branch are gone, but the ${engine} engine was unreachable so no volumes were checked or dropped. Re-run tidy with the engine up to clear fs3-${slug}_* volumes.`,
      { evidence: rescued ? [{ label: 'rescued observations', path: rescued.to }] : [{ label: 'tidy', none: true }] },
    );
  }
  return ctx.ok(payload, {
    evidence: rescued ? [{ label: 'rescued observations', path: rescued.to }] : [{ label: 'tidy', none: true }],
    next_action: `Tidy complete. The ordinal is free again — \`harness team new <slug> --propose\` will no longer count ${branch ?? slug}.`,
  });
}

export default defineExtension({
  name: 'team',
  summary: 'pij-team delivery lifecycle: mint a worktree + plan branch + next-ordinal plan folder, and tidy them away again when the plan lands.',
  verbs: {
    team: {
      summary: 'pij-team delivery scaffolding — `harness team <new|tidy>`.',
      sub: {
        new: {
          summary: 'Create the worktree, plan branch and next-ordinal plan folder for a new pij-team plan.',
          description:
            'Scans every worktree AND every local branch head for the highest NNN- plan ordinal, then creates ../fs3-<slug> on branch <ord>-<slug>, scaffolds docs/plans/<ord>-<slug>/ inside it, and seeds the four pij-team templates with their .dd.md siblings built. --propose computes and prints the same plan without creating anything.',
          args: [{ name: '<slug>', description: 'kebab-case plan slug, e.g. conversation-ingest' }],
          options: [
            {
              flags: '--propose',
              description: 'compute the ordinal and print what would be created, without creating a worktree, a branch or a plan folder',
            },
          ],
          run: runNew,
        },
        tidy: {
          summary: 'Remove the worktree, plan branch and slug-scoped docker volumes for a finished pij-team plan.',
          description:
            'The teardown counterpart to `new`. Rescues the worktree\'s observation buffer to the main clone FIRST (sha-verified) — before any mutation and before any refusal — then removes ../fs3-<slug>, prunes the registry, deletes the <ord>-<slug> branch, and drops zero-link fs3-<slug>_* docker volumes by name. Refuses on uncommitted changes, unpushed commits or an unmerged branch, always naming what would be lost. --dry-run (alias --propose) prints the same plan without touching anything.',
          args: [{ name: '<slug>', description: 'the kebab-case slug the worktree was created with, e.g. conversation-ingest' }],
          options: [
            {
              flags: '--dry-run',
              description: 'print what would be removed, rescued and refused, without touching anything',
            },
            {
              flags: '--propose',
              description: 'alias for --dry-run, matching `team new --propose`',
            },
            {
              flags: '--force',
              description: 'remove despite uncommitted changes, unpushed commits or an unmerged branch (they are still listed first)',
            },
            {
              flags: '--remote',
              description: 'also delete the remote branch (origin/<ord>-<slug>) after the local one',
            },
          ],
          run: runTidy,
        },
      },
    },
  },
});
