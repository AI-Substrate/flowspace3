-- Migration 0002 - the element kinds the tree model actually has.
--
-- 0001 pinned `kind IN ('callable', 'type', 'section')`, the flat three-category
-- model. The scanner now returns a TREE whose root is the file itself, and the
-- categories are `file | container | function | section`: `callable` was renamed
-- to `function`, `type` widened to `container` (a module parents what is inside
-- it, so it is an element rather than an invisible scope), and `file` is new.
--
-- 0001 is applied and its checksum is recorded, so it must never be edited.
-- Rewriting the CHECK is therefore a new migration, and the rename of existing
-- rows comes with it: an applied database that already holds `callable` rows
-- would fail to add the new constraint otherwise.

UPDATE elements SET kind = 'function'  WHERE kind = 'callable';
UPDATE elements SET kind = 'container' WHERE kind = 'type';

ALTER TABLE elements DROP CONSTRAINT IF EXISTS elements_kind_known;

ALTER TABLE elements
    ADD CONSTRAINT elements_kind_known
    CHECK (kind IN ('file', 'container', 'function', 'section'));
