-- Migration 0017 — deterministic-document rows and their inverse file index.
--
-- Ddoc rows remain ordinary elements. Their typed metadata is additive JSONB;
-- code and conversation elements keep NULL and pay no document-shaped columns.
ALTER TABLE elements DROP CONSTRAINT elements_kind_known;
ALTER TABLE elements
    ADD CONSTRAINT elements_kind_known
    CHECK (kind IN ('file', 'container', 'function', 'section', 'turn', 'row'));

ALTER TABLE elements ADD COLUMN ddoc JSONB;

CREATE INDEX elements_ddoc_id_kind_idx
    ON elements ((ddoc->>'id_kind'))
    WHERE ddoc IS NOT NULL;
CREATE INDEX elements_ddoc_gate_idx
    ON elements ((ddoc->>'gate_terminal'))
    WHERE ddoc IS NOT NULL;

-- File edges are derived from dd's corpus graph. Cascading from the owning row
-- makes replacement and content collection leave no orphaned inverse entries.
CREATE TABLE ddoc_file_refs (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    element_id  BIGINT NOT NULL REFERENCES elements(id) ON DELETE CASCADE,
    target_path TEXT NOT NULL,
    rel         TEXT NOT NULL,
    location    TEXT NOT NULL,

    UNIQUE (element_id, target_path, rel, location)
);

CREATE INDEX ddoc_file_refs_target_idx ON ddoc_file_refs (target_path);
