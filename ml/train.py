"""
Train an LSTM to predict the Kp index at multiple horizons (3h/6h/12h/24h).

Walk-forward validation: last 2 years split into 4 folds (~6 months each).
For each fold the model is retrained from scratch on the expanding window of
all data that precedes the fold, then evaluated on the fold.
After validation a final model is trained on all available data and saved.

Usage: python ml/train.py
Output: ml/models/kp_lstm.pt
"""

import io
import logging
import sys
from pathlib import Path

import numpy as np
import pandas as pd
import torch
import torch.nn as nn
import torch.nn.functional as F
from torch.utils.data import DataLoader, TensorDataset

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger(__name__)

# ── Paths ─────────────────────────────────────────────────────────────────────

PARQUET = Path(__file__).parent.parent / "data" / "kp_processed.parquet"
MODEL_DIR = Path(__file__).parent / "models"
MODEL_OUT = MODEL_DIR / "kp_lstm.pt"

# ── Feature schema ──────────────────────────────────────────────────────────────

SEQ_LEN = 16          # 16 × 3 h = 48 h lookback window

# Kp-unit features - all scaled by KP_MAX
KP_SCALED_FEATURES = [
    "kp",
    "lag_1", "lag_2", "lag_3", "lag_4", "lag_5", "lag_6", "lag_7",
    "kp_24h_max", "kp_72h_mean",
]
# Cyclical time encodings - already in [-1, 1], used as-is
TIME_FEATURES = [
    "hour_sin", "hour_cos",
    "month_sin", "month_cos",
    "solar_cycle_phase_sin", "solar_cycle_phase_cos",
]
# Physics drivers - min-max normalised with constants saved in the checkpoint
MINMAX_FEATURES = ["f107_adj", "sn", "f107_1d_delta"]

FEATURES = KP_SCALED_FEATURES + TIME_FEATURES + MINMAX_FEATURES
N_FEATURES = len(FEATURES)

# ── Normalisation constants ───────────────────────────────────────────────────

KP_MAX = 9.0          # Kp is bounded; simple linear scale to [0, 1]
# Robust upper quantile used to cap rare flare-day flux/sunspot spikes before
# min-max scaling, so common values keep usable resolution.
ROBUST_QUANTILE = 0.995

# ── Forecast horizons ───────────────────────────────────────────────────────────

HORIZON_HOURS = [3, 6, 12, 24]
HORIZON_PERIODS = [1, 2, 4, 8]            # periods ahead (1 period = 3 h)
HORIZON_WEIGHTS = [1.0, 0.8, 0.6, 0.4]    # closer horizons dominate training
# What each head predicts: the change from the newest reading, or the level.
# See the comment beside the target construction for the measurement behind it.
HORIZON_TARGETS = ["residual", "residual", "level", "level"]
MAX_HORIZON = max(HORIZON_PERIODS)
N_HORIZONS = len(HORIZON_HOURS)

# ── Hyperparameters ───────────────────────────────────────────────────────────

HIDDEN = 64
N_LAYERS = 2
DROPOUT = 0.2
BATCH_SIZE = 512
LR = 1e-3
MAX_EPOCHS = 60
PATIENCE = 7          # early stopping

# Walk-forward: last 2 years, 4 folds of ~6 months each
N_FOLDS = 4
FOLD_PERIODS = 1460   # 1460 × 3 h ≈ 182 days ≈ 6 months

DEVICE = torch.device("cpu")
_WEIGHTS_T = torch.tensor(HORIZON_WEIGHTS, dtype=torch.float32)


# ── Model ─────────────────────────────────────────────────────────────────────

class KpLSTM(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.lstm = nn.LSTM(
            input_size=N_FEATURES,
            hidden_size=HIDDEN,
            num_layers=N_LAYERS,
            batch_first=True,
            dropout=DROPOUT if N_LAYERS > 1 else 0.0,
        )
        self.head = nn.Sequential(
            nn.Linear(HIDDEN, 32),
            nn.ReLU(),
            nn.Linear(32, N_HORIZONS),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        out, _ = self.lstm(x)
        return self.head(out[:, -1, :])   # (B, N_HORIZONS)


# ── Normalisation ─────────────────────────────────────────────────────────────

def compute_minmax(df: pd.DataFrame) -> dict[str, tuple[float, float]]:
    """Robust (clipped) min-max ranges for the physics features."""
    f107 = df["f107_adj"]
    sn = df["sn"]
    delta = df["f107_1d_delta"]
    delta_cap = float(delta.abs().quantile(ROBUST_QUANTILE))
    return {
        "f107_adj": (float(f107.min()), float(f107.quantile(ROBUST_QUANTILE))),
        "sn": (0.0, float(sn.quantile(ROBUST_QUANTILE))),
        "f107_1d_delta": (-delta_cap, delta_cap),
    }


def normalize_features(df: pd.DataFrame, minmax: dict[str, tuple[float, float]]) -> np.ndarray:
    """Build the (T, N_FEATURES) model-input matrix in FEATURES order."""
    cols: list[np.ndarray] = []
    for feat in FEATURES:
        raw = df[feat].to_numpy(dtype=np.float32)
        if feat in KP_SCALED_FEATURES:
            col = raw / KP_MAX
        elif feat in minmax:
            lo, hi = minmax[feat]
            col = (np.clip(raw, lo, hi) - lo) / (hi - lo)
        else:  # time features, already bounded
            col = raw
        cols.append(col.astype(np.float32))
    return np.stack(cols, axis=1)


# ── Data helpers ──────────────────────────────────────────────────────────────

def make_sequences(
    values: np.ndarray,      # (T, N_FEATURES), already normalised
    targets: np.ndarray,     # (T, N_HORIZONS)
) -> tuple[torch.Tensor, torch.Tensor]:
    """Sliding-window sequences of length SEQ_LEN with multi-horizon targets."""
    n = len(values) - SEQ_LEN
    X = np.stack([values[i : i + SEQ_LEN] for i in range(n)])
    y = targets[SEQ_LEN:]
    return (
        torch.tensor(X, dtype=torch.float32),
        torch.tensor(y, dtype=torch.float32),
    )


def loader(X: torch.Tensor, y: torch.Tensor, shuffle: bool) -> DataLoader:
    return DataLoader(TensorDataset(X, y), batch_size=BATCH_SIZE, shuffle=shuffle)


def weighted_huber(pred: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    """Mean loss across horizons, weighted so nearer horizons dominate.

    In effect this is MSE, not Huber, and it always has been. `F.huber_loss`
    defaults to delta=1.0 while targets are Kp/9 in [0, 1], so every residual
    falls in the quadratic branch: measured 2026-09-01 over the walk-forward
    window, mean |residual| 0.096 in scaled units, p99 0.409, max 0.832, and
    0.0000 percent above delta.

    Deliberately left as it is. Lowering delta so the robust branch engages would
    downweight large errors, and in this data the large errors are the storms:
    at delta 0.15, 84.5 percent of Kp >= 5 residuals fall in the linear branch
    against 19 percent of all residuals. Robustness here means caring less about
    the tail the model already under-fits (AUD-031). The name is kept because it
    is what the checkpoint and the papers call it; the docstring is the honest
    part.
    """
    per_h = F.huber_loss(pred, target, reduction="none").mean(dim=0)  # (N_HORIZONS,)
    return (per_h * _WEIGHTS_T).sum() / _WEIGHTS_T.sum()


# ── Training loop ─────────────────────────────────────────────────────────────

def train_model(
    train_X: torch.Tensor,
    train_y: torch.Tensor,
    val_X: torch.Tensor,
    val_y: torch.Tensor,
) -> KpLSTM:
    model = KpLSTM().to(DEVICE)
    opt = torch.optim.Adam(model.parameters(), lr=LR)
    scheduler = torch.optim.lr_scheduler.ReduceLROnPlateau(opt, patience=3, factor=0.5)

    tr_loader = loader(train_X, train_y, shuffle=True)
    va_loader = loader(val_X, val_y, shuffle=False)

    best_val, best_state, wait = float("inf"), None, 0

    for epoch in range(1, MAX_EPOCHS + 1):
        model.train()
        for xb, yb in tr_loader:
            opt.zero_grad()
            weighted_huber(model(xb), yb).backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()

        model.eval()
        with torch.no_grad():
            val_preds = torch.cat([model(xb) for xb, _ in va_loader])
            val_loss = weighted_huber(val_preds, val_y).item()

        scheduler.step(val_loss)

        if val_loss < best_val:
            best_val, best_state, wait = val_loss, {k: v.clone() for k, v in model.state_dict().items()}, 0
        else:
            wait += 1
            if wait >= PATIENCE:
                log.info("  early stop at epoch %d (best val loss %.5f)", epoch, best_val)
                break

    model.load_state_dict(best_state)
    return model


# ── Metrics ───────────────────────────────────────────────────────────────────

def evaluate(model: KpLSTM, X: torch.Tensor, y: torch.Tensor) -> dict[str, dict[str, float]]:
    """Per-horizon RMSE and MAE in original Kp units, keyed by horizon label."""
    model.eval()
    with torch.no_grad():
        preds = torch.cat([
            model(X[i : i + BATCH_SIZE])
            for i in range(0, len(X), BATCH_SIZE)
        ]).numpy()
    truth = y.numpy()
    preds_kp = preds * KP_MAX
    truth_kp = truth * KP_MAX
    out: dict[str, dict[str, float]] = {}
    for k, hours in enumerate(HORIZON_HOURS):
        err = preds_kp[:, k] - truth_kp[:, k]
        out[f"{hours}h"] = {
            "rmse": float(np.sqrt(np.mean(err ** 2))),
            "mae": float(np.mean(np.abs(err))),
        }
    return out


def mean_metric(metrics: list[dict[str, dict[str, float]]], label: str, key: str) -> float:
    return float(np.mean([m[label][key] for m in metrics]))


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    if not PARQUET.exists():
        log.error("Parquet not found - run preprocess.py first: %s", PARQUET)
        sys.exit(1)

    log.info("Loading %s", PARQUET)
    df = pd.read_parquet(PARQUET).sort_values("timestamp").reset_index(drop=True)
    log.info("Rows: %d  |  Date range: %s → %s", len(df), df.timestamp.iloc[0], df.timestamp.iloc[-1])

    minmax = compute_minmax(df)
    log.info("Physics min-max (robust q=%.3f): %s", ROBUST_QUANTILE,
             {k: (round(v[0], 1), round(v[1], 1)) for k, v in minmax.items()})

    values = normalize_features(df, minmax)               # (T, N_FEATURES)
    kp_norm = df["kp"].to_numpy(dtype=np.float32) / KP_MAX

    # Multi-horizon targets: the CHANGE in Kp at +1, +2, +4, +8 periods, not the
    # level. Measured 2026-09-01: with a level target the model loses to
    # persistence at 3h and ties at 6h, and its predictions are systematically
    # flatter than reality, sd(pred)/sd(obs) running 0.72 at 3h down to 0.33 at
    # 24h. A squared-error loss on the level fits the conditional mean, so with a
    # weak signal it shrinks toward the unconditional one, and at short leads
    # persistence is close to optimal so any shrinkage costs.
    #
    # Predicting the change makes persistence the zero of the target: the model
    # can only improve on it rather than having to rediscover it. The error is
    # unchanged by the reparameterisation, because the same constant is added to
    # prediction and target, so `evaluate` below and the stored validation
    # metrics stay directly comparable to the level-target runs.
    # Per horizon, because the right parameterisation is not the same at both
    # ends and the crossover was measured rather than guessed. sd of the change
    # target against sd of the level target, in sample: 0.91 vs 1.41 at 3h, 1.16
    # vs 1.41 at 6h, 1.42 vs 1.41 at 12h, 1.63 vs 1.41 at 24h. The change is the
    # easier target at short leads and the harder one at long leads, and it
    # crosses over at 12h.
    #
    # Confirmed by training residual-for-all on 2026-09-01: 3h improved 15.5
    # percent and 6h 3.1 percent on the walk-forward window, while 12h regressed
    # 4.8 percent and 24h 8.5 percent. Predicting the change makes persistence
    # the zero of the target, which is what a short lead wants; at 24h
    # persistence is a poor forecast and the mean is a good one, so a change
    # target makes the model work for something the level target gave it free.
    # `make_sequences` pairs X[i] = values[i : i+SEQ_LEN] with y[i] =
    # targets[i+SEQ_LEN], so the newest observation in the window is slot
    # i+SEQ_LEN-1 and the target row is i+SEQ_LEN. A lead of p periods from the
    # newest observation is therefore kp[t-1+p] at target row t, which is a roll
    # of -(p-1), not -p.
    #
    # It was -p until 2026-09-01, which put every head one period beyond its
    # label: the head sold as 3h was trained on 6h, and the one sold as 24h on
    # 27h (AUD-032). Every skill number measured before that date, including the
    # validation metrics inside the shipped checkpoint, describes a horizon the
    # service does not publish.
    base = np.roll(kp_norm, 1)          # kp at the newest observation, kp[t-1]
    targets = np.stack(
        [
            np.roll(kp_norm, -(p - 1)) - base
            if mode == "residual"
            else np.roll(kp_norm, -(p - 1))
            for p, mode in zip(HORIZON_PERIODS, HORIZON_TARGETS)
        ],
        axis=1,
    )  # (T, 4), in units of Kp/KP_MAX

    # Arithmetic, so it is checkable without a model. For a window whose newest
    # observation is row L, head k must be trained on kp[L + HORIZON_PERIODS[k]].
    # `np.roll` wraps, and row 0 of `base` holds the last element of the series,
    # but y indices start at SEQ_LEN so row 0 is never a target.
    _probe = 100
    _last = _probe + SEQ_LEN - 1
    for _k, (_p, _mode) in enumerate(zip(HORIZON_PERIODS, HORIZON_TARGETS)):
        _want = kp_norm[_last + _p] - (kp_norm[_last] if _mode == "residual" else 0.0)
        _got = targets[_probe + SEQ_LEN, _k]
        assert abs(_want - _got) < 1e-6, (
            f"head {_k} ({HORIZON_HOURS[_k]}h) is trained on the wrong lead: "
            f"expected kp[{_last + _p}], target does not match"
        )
    # Drop the trailing rows whose furthest horizon would wrap around.
    values = values[:-MAX_HORIZON]
    targets = targets[:-MAX_HORIZON]

    T = len(values)
    val_start = T - N_FOLDS * FOLD_PERIODS   # index where validation window begins

    if val_start <= SEQ_LEN:
        log.error("Not enough training data before validation window")
        sys.exit(1)

    val_cutoff_ts = df.timestamp.iloc[val_start + SEQ_LEN]
    log.info(
        "Walk-forward: %d folds × ~%d periods (~6 months each) | val start: %s | horizons: %s",
        N_FOLDS, FOLD_PERIODS, val_cutoff_ts.date(), HORIZON_HOURS,
    )

    # ── Walk-forward validation ───────────────────────────────────────────────
    fold_metrics: list[dict[str, dict[str, float]]] = []

    for fold in range(N_FOLDS):
        fold_start = val_start + fold * FOLD_PERIODS
        fold_end   = fold_start + FOLD_PERIODS

        tr_vals, tr_tgt = values[:fold_start], targets[:fold_start]
        fo_vals, fo_tgt = values[fold_start:fold_end], targets[fold_start:fold_end]

        train_X, train_y = make_sequences(tr_vals, tr_tgt)
        fold_X,  fold_y  = make_sequences(fo_vals, fo_tgt)

        # Reserve last 10% of training window as an internal val set for early stopping
        split = int(len(train_X) * 0.9)
        val_X_es, val_y_es = train_X[split:], train_y[split:]
        train_X, train_y   = train_X[:split], train_y[:split]

        ts_start = df.timestamp.iloc[fold_start + SEQ_LEN].date()
        ts_end   = df.timestamp.iloc[min(fold_end - 1, T - 1)].date()
        log.info(
            "Fold %d/%d  [%s → %s]  train=%d  test=%d",
            fold + 1, N_FOLDS, ts_start, ts_end, len(train_X), len(fold_X),
        )

        model = train_model(train_X, train_y, val_X_es, val_y_es)
        metrics = evaluate(model, fold_X, fold_y)
        fold_metrics.append(metrics)
        log.info(
            "  Fold %d  %s",
            fold + 1,
            "  ".join(f"{h}:RMSE={metrics[h]['rmse']:.3f}/MAE={metrics[h]['mae']:.3f}"
                      for h in (f"{x}h" for x in HORIZON_HOURS)),
        )

    val_summary = {
        f"{h}h": {
            "rmse": mean_metric(fold_metrics, f"{h}h", "rmse"),
            "mae": mean_metric(fold_metrics, f"{h}h", "mae"),
        }
        for h in HORIZON_HOURS
    }
    mean_rmse = float(np.mean([v["rmse"] for v in val_summary.values()]))
    mean_mae  = float(np.mean([v["mae"] for v in val_summary.values()]))
    log.info("Walk-forward per-horizon mean: %s",
             {h: (round(v["rmse"], 3), round(v["mae"], 3)) for h, v in val_summary.items()})
    log.info("Walk-forward overall  mean RMSE=%.4f  mean MAE=%.4f", mean_rmse, mean_mae)

    # ── Final model: train on all data ────────────────────────────────────────
    log.info("Training final model on all %d rows", T)
    all_X, all_y = make_sequences(values, targets)
    split = int(len(all_X) * 0.95)
    final_model = train_model(all_X[:split], all_y[:split], all_X[split:], all_y[split:])

    final_metrics = evaluate(final_model, all_X[split:], all_y[split:])
    log.info("Final model hold-out: %s",
             {h: (round(v["rmse"], 3), round(v["mae"], 3)) for h, v in final_metrics.items()})

    # ── Save ─────────────────────────────────────────────────────────────────
    MODEL_DIR.mkdir(parents=True, exist_ok=True)
    checkpoint = {
            "model_state": final_model.state_dict(),
            "hyperparams": {
                "seq_len": SEQ_LEN,
                "features": FEATURES,
                "n_features": N_FEATURES,
                "hidden": HIDDEN,
                "n_layers": N_LAYERS,
                "dropout": DROPOUT,
                "kp_max": KP_MAX,
                "horizons": HORIZON_HOURS,
                "horizon_periods": HORIZON_PERIODS,
                "kp_scaled_features": KP_SCALED_FEATURES,
                # What the head emits. "residual" means the output is the change
                # from the newest reading and the caller adds it back; "level"
                # was the parameterisation before 2026-09-01. serve.py reads this
                # and defaults to "level", so an older checkpoint still loads.
                "target": HORIZON_TARGETS,
                "minmax": {k: list(v) for k, v in minmax.items()},
                "feature_defaults": {
                    "f107_adj": float(df["f107_adj"].mean()),
                    "sn": float(df["sn"].mean()),
                    "f107_1d_delta": 0.0,
                },
            },
            "validation": {
                "n_folds": N_FOLDS,
                "fold_periods": FOLD_PERIODS,
                "per_fold": fold_metrics,
                "per_horizon": val_summary,
                "mean_rmse": mean_rmse,
                "mean_mae": mean_mae,
            },
            "trained_through": str(df.timestamp.iloc[-1].date()),
    }

    # serve.py loads with weights_only=True, whose restricted unpickler accepts
    # tensors and builtins and nothing else. A numpy scalar reaching the
    # metadata would not fail here, it would fail when the ML service next
    # started in production. So do exactly what serve.py will do, in memory,
    # before anything is written. AUD-010.
    #
    # This deliberately is not a json.dumps round-trip. np.float64 subclasses
    # float, so json accepts it while the restricted unpickler refuses it, and
    # np.float64 is precisely what np.mean() and np.sqrt() return in evaluate().
    # A json check would pass the one case most likely to occur.
    buffer = io.BytesIO()
    torch.save(checkpoint, buffer)
    buffer.seek(0)
    try:
        torch.load(buffer, map_location="cpu", weights_only=True)
    except Exception as exc:
        raise TypeError(
            "checkpoint does not load under weights_only=True, so the ML "
            f"service would refuse to start with it: {exc}. Wrap numpy values "
            "in float() or int() before saving."
        ) from exc

    torch.save(checkpoint, MODEL_OUT)
    log.info("Saved model → %s", MODEL_OUT)


if __name__ == "__main__":
    main()
