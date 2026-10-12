"""Pure pieces of the extractor (no database): vector decoding, credential-free identity, message keys."""

import struct

import numpy as np
import pytest

from conv_atlas import extract


def test_pgvector_binary_send_format_decodes():
    vals = [0.5, -1.25, 3.0]
    raw = struct.pack(">hh", len(vals), 0) + struct.pack(f">{len(vals)}f", *vals)
    out = extract._vec(raw)
    assert out.dtype == np.float32 and out.tolist() == vals
    assert extract._vec(None) is None


def test_db_identity_never_carries_credentials():
    ident = extract.db_identity("postgres://user:hunter2@127.0.0.1:5433/flowspace3")
    assert ident == {"host": "127.0.0.1:5433", "name": "flowspace3"}
    assert "hunter2" not in repr(ident)


# longer than the 40-char key, as real messages are: trailing quotes/EOF/closing tags fall outside it
BODY = 'Ship it: the "gate" is green\nnext step is merge into main today'


@pytest.mark.parametrize(
    "send",
    [
        '[tool-call pij_send]\n{"to":"pij-calm-otter","message":"Ship it: the \\"gate\\" is green\\nnext step is '
        'merge into main today"}',
        '[tool-call Bash]\n{"command":"pij send pij-calm-otter \\"Ship it: the \\\\\\"gate\\\\\\" is green\\nnext '
        'step is merge into main today\\""}',
        '[tool-call bash]\n{"command":"cd /x && pij send pij-calm-otter --body-file - <<\'EOF\'\\nShip it: the '
        '\\"gate\\" is green\\nnext step is merge into main today\\nEOF"}',
        '[tool-call Bash]\n{"command":"pij send --wait pij-calm-otter \\"$(cat <<\'EOF\'\\nShip it: the gate is green'
        '\\nnext step is merge into main today\\nEOF\\n)\\""}',
    ],
    ids=["pij_send-json", "cli-quoted", "cli-body-file-heredoc", "cli-cat-heredoc"],
)
def test_a_send_call_and_its_delivered_peer_turn_share_one_key(send):
    omp = f"[pij from pij-brave-heron] {BODY}"
    claude = (
        'Another Claude session sent a message:\n<cross-session-message from="uds:/tmp/x.sock" '
        f'from-name="pij-brave-heron">\n[pij-rs from pij-brave-heron]\n{BODY}\n</cross-session-message>'
    )
    key = extract.message_key(omp, send=False)
    assert key == "shipitthegateisgreennextstepismergeintom"
    assert extract.message_key(claude, send=False) == key
    assert extract.message_key(send, send=True) == key


def test_short_or_envelope_less_bodies_have_no_key():
    assert extract.message_key("[pij from pij-brave-heron] ack", send=False) is None, "too short to be distinctive"
    assert extract.message_key("<task-notification>done</task-notification>", send=False) is None
    assert extract.message_key('[tool-call Bash]\n{"command":"pij state pij-brave-heron"}', send=True) is None
