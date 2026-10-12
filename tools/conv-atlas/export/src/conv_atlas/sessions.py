"""Long-session detail: window novelty, topic-shift points and a per-session 2-D layout.

Pure numpy over one session's work-turn vectors (in turn order). Vectors are
centred on the corpus mean first; raw embedding cosine is inflated by anisotropy.
"""

from __future__ import annotations

import numpy as np

WINDOW = 25
MAX_SHIFTS = 12
SHIFT_QUANTILE = 0.9


def _unit(v: np.ndarray) -> np.ndarray:
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.where(n == 0, 1, n)


def novelty(vectors: np.ndarray, mean: np.ndarray, window: int = WINDOW) -> np.ndarray:
    """1 - cos(mean of the previous `window` turns, mean of the next `window`), per turn.

    Turns closer than `window` to either end have no full window and score 0.
    """
    m = len(vectors)
    out = np.zeros(m, dtype=np.float64)
    if m < 2 * window + 1:
        return out
    v = _unit(vectors.astype(np.float64) - mean)
    csum = np.vstack([np.zeros((1, v.shape[1])), np.cumsum(v, axis=0)])
    k = np.arange(window, m - window + 1)
    prev = _unit(csum[k] - csum[k - window])
    nxt = _unit(csum[k + window] - csum[k])
    out[k] = 1 - np.sum(prev * nxt, axis=1)
    return out


def shifts(series: np.ndarray, window: int = WINDOW, max_shifts: int = MAX_SHIFTS) -> list[int]:
    """Indices of change points: the highest novelty peaks, at least 2*window apart, above the series' p90."""
    live = series[series > 0]
    if live.size == 0:
        return []
    floor = float(np.quantile(live, SHIFT_QUANTILE))
    picked: list[int] = []
    for k in np.argsort(-series, kind="stable"):
        if series[k] < floor or len(picked) >= max_shifts:
            break
        if all(abs(int(k) - p) >= 2 * window for p in picked):
            picked.append(int(k))
    return sorted(picked)


def layout(vectors: np.ndarray) -> np.ndarray:
    """Deterministic 2-D PCA of a session's turn vectors; component signs fixed by their largest loading."""
    x = vectors.astype(np.float64)
    if len(x) < 3:
        return np.zeros((len(x), 2))
    x = x - x.mean(axis=0)
    _, _, vt = np.linalg.svd(x, full_matrices=False)
    comps = vt[:2]
    signs = np.sign(comps[np.arange(2), np.argmax(np.abs(comps), axis=1)])
    return x @ (comps * signs[:, None]).T
