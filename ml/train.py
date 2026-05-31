"""
Train an LSTM to predict the Kp index at multiple horizons (3h/6h/12h/24h).

Walk-forward validation: last 2 years split into 4 folds (~6 months each).
For each fold the model is retrained from scratch on the expanding window of
all data that precedes the fold, then evaluated on the fold.
After validation a final model is trained on all available data and saved.

Usage: python ml/train.py
Output: ml/models/kp_lstm.pt
"""

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
    """Mean HuberLoss across horizons, weighted so nearer horizons dominate."""
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

    # Multi-horizon targets: Kp at +1, +2, +4, +8 periods (3h/6h/12h/24h ahead).
    targets = np.stack([np.roll(kp_norm, -p) for p in HORIZON_PERIODS], axis=1)  # (T, 4)
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
    torch.save(
        {
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
        },
        MODEL_OUT,
    )
    log.info("Saved model → %s", MODEL_OUT)


if __name__ == "__main__":
    main()
