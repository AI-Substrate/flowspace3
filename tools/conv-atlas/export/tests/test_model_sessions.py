"""Long-session detail: novelty finds a planted topic change; the per-session layout is deterministic."""

import numpy as np

from conv_atlas import sessions


def _session(change_at: int = 120, n: int = 260, dim: int = 24, seed: int = 0):
    rng = np.random.default_rng(seed)
    a, b = rng.normal(size=(2, dim))
    base = np.where(np.arange(n)[:, None] < change_at, a, b)
    v = base + rng.normal(scale=0.8, size=(n, dim))
    return v / np.linalg.norm(v, axis=1, keepdims=True), np.zeros(dim)


def test_novelty_peaks_at_the_planted_change():
    v, mean = _session()
    nov = sessions.novelty(v, mean, 25)
    assert abs(int(np.argmax(nov)) - 120) <= 3
    assert nov[:25].sum() == 0 and nov[-24:].sum() == 0, "no full window at the edges"
    assert nov.min() >= 0 and nov.max() <= 2


def test_shifts_find_the_change_and_respect_spacing():
    v, mean = _session()
    picked = sessions.shifts(sessions.novelty(v, mean, 25), 25)
    assert any(abs(k - 120) <= 3 for k in picked)
    assert all(b - a >= 50 for a, b in zip(picked, picked[1:], strict=False))


def test_short_session_has_no_novelty_or_shifts():
    v, mean = _session(change_at=10, n=40)
    nov = sessions.novelty(v, mean, 25)
    assert not nov.any() and sessions.shifts(nov, 25) == []


def test_session_layout_is_deterministic_and_sign_stable():
    v, _ = _session()
    a, b = sessions.layout(v), sessions.layout(v.copy())
    assert np.array_equal(a, b)
    assert np.allclose(sessions.layout(-v), -a) or np.allclose(sessions.layout(-v), a)
    assert a.shape == (len(v), 2)
