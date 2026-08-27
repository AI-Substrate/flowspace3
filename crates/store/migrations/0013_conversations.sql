-- Migration 0013 - conversations: turns as first-class indexed content.
--
-- Workshop 005 (req-0024..0027). These two tables are REF layer: pointers and
-- provenance. The turn TEXT itself flows into the EXISTING content layer
-- (elements -> smart_content -> embeddings) through `turns.blob_sha`, so
-- conversations get the same summaries, the same vectors, the same dedupe and
-- the same three-level GC as code. There is no parallel conversation pipeline,
-- and that is decision C1: conversations are a content TYPE, not a product.
--
-- The bridge is one column. `turns.blob_sha` is the content address of the
-- turn's canonical stored form, which is also the `blob_sha` and `raw_text` of
-- an `elements` row (kind = 'turn'). Two consequences the rest of the system
-- gets for free:
--
--   * two byte-identical turns - in one conversation or in forty - share ONE
--     paid summary and ONE pair of vectors, because enrichment is keyed by
--     raw_hash (0004, decision D2) and agents repeat themselves constantly.
--   * "is this content still referenced?" gains a second true answer. Before
--     this migration a raw hash was referenced only while a registered
--     worktree held a blob carrying it; now a stored turn is equally a root of
--     reference. `fs3_store::roots` carries that predicate at all five sites.

-- ═══ CONVERSATIONS ═══════════════════════════════════════════════════════
--
-- The anchor is a POINTER, NOT OWNERSHIP (workshop 005 OQ2, ruled by Jordan
-- via o-prime 2026-08-27), and that ruling is why `repo_identity` is TEXT with
-- NO foreign key to repos(id). The sketch in the workshop had a real FK, and
-- it cannot hold: `fs3_store::roots::remove_root` deletes the repos row once
-- its last worktree goes, so a single anchored conversation would turn every
-- `flowspace3 remove` of that repository into a foreign-key violation - the
-- exact opposite of "conversations outlive the repo".
--
-- Storing the IDENTITY instead buys two more things a surrogate key cannot:
-- the anchor survives the removal AND re-links itself if the repository is
-- ever added back (repos.identity is UNIQUE, 0003), and a conversation can be
-- anchored to a repository fs3 has never been asked to index at all.
--
-- The cost, recorded the way 0004 records its D7: there is no referential
-- integrity on this column. A typo in an identity is a conversation that no
-- anchor filter will ever match, and the database will not say so. That is the
-- price of a pointer, and it is the same bargain the content layer already
-- makes with `blob_sha` - a value, deliberately not a foreign key, which is
-- what lets forty checkouts share one parse.
CREATE TABLE conversations (
    -- Caller-supplied or minted at import.
    guid          UUID        PRIMARY KEY,
    -- Anchor: repos.identity as text. See above - deliberately not an FK.
    repo_identity TEXT,
    -- Anchor: the checkout path, within or beside the repository.
    worktree      TEXT,
    -- Anchor: the commit the conversation started from.
    base_sha      TEXT,
    -- Optional; import may derive it from the first turn.
    title         TEXT,
    started_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "Conversations about this repo as it was then" (workshop 005) - the anchor
-- filter that `search --source conversation --repo X` and `conversation list`
-- both narrow on, and the join the search CTE reaches through.
CREATE INDEX conversations_repo_identity_idx ON conversations (repo_identity);

-- ═══ TURNS ═══════════════════════════════════════════════════════════════
--
-- Sequence is to a conversation what hierarchy is to code and sections are to
-- markdown (req-0026): `turn_no` is dense from 1 and IS the axis every
-- navigation walks, which is why it is half the primary key rather than an
-- ordering column beside a surrogate id. A windowed read (-10/+20 around a
-- hit) is then a range scan on the primary key and nothing else.
CREATE TABLE turns (
    conversation_id UUID        NOT NULL REFERENCES conversations(guid) ON DELETE CASCADE,
    -- Dense 1..N. The navigation axis.
    turn_no         INT         NOT NULL,
    role            TEXT        NOT NULL,
    -- Decision C8: measured, peer-injected turns EQUAL human turns in count in
    -- an orchestrated fleet, so a role-only model would report an agent fleet
    -- as half-human.
    source          TEXT        NOT NULL,
    -- Decision C6: repo HEAD at time-of-turn. Tool output is stored as a
    -- truncated head, and truncation is only honest if the state it came from
    -- is addressable.
    head_sha        TEXT,
    at              TIMESTAMPTZ NOT NULL,
    -- The turn's prose, verbatim. The gold, and modest in volume.
    body            TEXT        NOT NULL,
    -- Typed sub-items (req-0025): tool calls and their results, already shaped
    -- by the intake policy. JSONB so a new item kind is a code change and
    -- never a migration.
    items           JSONB       NOT NULL DEFAULT '[]',
    -- Content address of the canonical stored form: THE bridge into the
    -- element/content layer, and the join key GC and the spend guard ask about.
    blob_sha        TEXT        NOT NULL,

    PRIMARY KEY (conversation_id, turn_no),

    CONSTRAINT turns_role_known   CHECK (role IN ('human', 'agent')),
    CONSTRAINT turns_source_known CHECK (source IN ('human', 'peer', 'system')),
    -- Dense from 1, matching the 1-based span a turn element carries.
    CONSTRAINT turns_ordinal_positive CHECK (turn_no >= 1)
);

-- The reference predicate's index. `roots.rs` asks "does any stored turn carry
-- this blob?" once per GC level and once per job at the point of spend, so the
-- blob -> turn direction is the hot one, exactly as worktree_files_blob_sha_idx
-- is for code.
CREATE INDEX turns_blob_sha_idx ON turns (blob_sha);

-- ═══ ELEMENTS: THE TURN KIND ═════════════════════════════════════════════
--
-- Widening the closed kind enum, kept under 0004's constraint NAME because it
-- is the same contract and the test that proves it bites cites that name.
-- Without this, `ElementKind::Turn` is a value Rust can hold and Postgres will
-- refuse, and the bridge above is unreachable from code.
ALTER TABLE elements DROP CONSTRAINT elements_kind_known;
ALTER TABLE elements
    ADD CONSTRAINT elements_kind_known
    CHECK (kind IN ('file', 'container', 'function', 'section', 'turn'));

-- Removing a conversation takes its turn elements with it, and the only thing
-- that identifies them as ITS turns is the address prefix: `blob_sha` is
-- shared by construction (that is the dedupe), so deleting by blob would take
-- another conversation's identical turn with it.
--
-- Partial, because turn rows are the only ones ever looked up this way: a
-- full index on `address` would carry every element in the database to answer
-- a question only conversations ask.
CREATE INDEX turn_elements_address_idx ON elements (address) WHERE kind = 'turn';
