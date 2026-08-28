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
      if (!fsWrite.copy(join(templateDir, name), folder, { confineRoot: worktree })) {
        throw new Error(`could not copy template ${name} into ${folder}`);
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

export default defineExtension({
  name: 'team',
  summary: 'Scaffold pij-team delivery: a worktree, its plan branch, and the next-ordinal plan folder.',
  verbs: {
    team: {
      summary: 'pij-team delivery scaffolding.',
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
      },
    },
  },
});
