/**
 * The repository-root half of the file-link fixture: `notes.dd.json` cites this
 * file as `src/library.ts`, anchored on the repository root rather than on the
 * citing document. It exists only to BE there, so that removing it in a temp
 * copy is a real change to the world rather than a change to an assertion.
 */
export const LIBRARY = 'library';
