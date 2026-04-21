"""
Train an LSTM to predict the next 3-hour Kp index value.

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

# ── Hyperparameters ───────────────────────────────────────────────────────────

SEQ_LEN = 16          # 16 × 3 h = 48 h lookback window
FEATURES = [
    "kp",
    "hour_sin", "hour_cos",
    "month_sin", "month_cos",
    "solar_cycle_phase_sin", "solar_cycle_phase_cos",
]
N_FEATURES = len(FEATURES)

HIDDEN = 64
N_LAYERS = 2
DROPOUT = 0.2
BATCH_SIZE = 512
LR = 1e-3
MAX_EPOCHS = 60
PATIENCE = 7          # early stopping

KP_MAX = 9.0          # Kp is bounded; simple linear scale to [0, 1]

# Walk-forward: last 2 years, 4 folds of ~6 months each
N_FOLDS = 4
FOLD_PERIODS = 1460   # 1460 × 3 h ≈ 182 days ≈ 6 months

DEVICE = torch.device("cpu")


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
            nn.Linear(32, 1),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        out, _ = self.lstm(x)
        return self.head(out[:, -1, :]).squeeze(1)


# ── Data helpers ──────────────────────────────────────────────────────────────

def make_sequences(
    values: np.ndarray,      # (T, N_FEATURES), already normalised
    targets: np.ndarray,     # (T,)
) -> tuple[torch.Tensor, torch.Tensor]:
    """Sliding-window sequences of length SEQ_LEN."""
    n = len(values) - SEQ_LEN
    X = np.stack([values[i : i + SEQ_LEN] for i in range(n)])
    y = targets[SEQ_LEN:]
    return (
        torch.tensor(X, dtype=torch.float32),
        torch.tensor(y, dtype=torch.float32),
    )


def loader(X: torch.Tensor, y: torch.Tensor, shuffle: bool) -> DataLoader:
    return DataLoader(TensorDataset(X, y), batch_size=BATCH_SIZE, shuffle=shuffle)


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
    loss_fn = nn.HuberLoss()   # less sensitive to storm spikes than MSE

    tr_loader = loader(train_X, train_y, shuffle=True)
    va_loader = loader(val_X, val_y, shuffle=False)

    best_val, best_state, wait = float("inf"), None, 0

    for epoch in range(1, MAX_EPOCHS + 1):
        model.train()
        for xb, yb in tr_loader:
            opt.zero_grad()
            loss_fn(model(xb), yb).backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()

        model.eval()
        with torch.no_grad():
            val_preds = torch.cat([model(xb) for xb, _ in va_loader])
            val_loss = loss_fn(val_preds, val_y).item()

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

def evaluate(model: KpLSTM, X: torch.Tensor, y: torch.Tensor) -> tuple[float, float]:
    """Return (RMSE, MAE) in original Kp units."""
    model.eval()
    with torch.no_grad():
        preds = torch.cat([
            model(X[i : i + BATCH_SIZE])
            for i in range(0, len(X), BATCH_SIZE)
        ]).numpy()
    truth = y.numpy()
    # de-normalise
    preds_kp = preds * KP_MAX
    truth_kp = truth * KP_MAX
    rmse = float(np.sqrt(np.mean((preds_kp - truth_kp) ** 2)))
    mae  = float(np.mean(np.abs(preds_kp - truth_kp)))
    return rmse, mae


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    if not PARQUET.exists():
        log.error("Parquet not found — run preprocess.py first: %s", PARQUET)
        sys.exit(1)

    log.info("Loading %s", PARQUET)
    df = pd.read_parquet(PARQUET).sort_values("timestamp").reset_index(drop=True)
    log.info("Rows: %d  |  Date range: %s → %s", len(df), df.timestamp.iloc[0], df.timestamp.iloc[-1])

    # Normalise to [0, 1]
    values = (df[FEATURES].to_numpy(dtype=np.float32)) / KP_MAX
    targets = (df["kp"].to_numpy(dtype=np.float32)) / KP_MAX

    # Target is the NEXT period's Kp (shift-1); drop the last row which has no target
    target_next = np.roll(targets, -1)
    values  = values[:-1]
    targets = target_next[:-1]

    T = len(values)
    val_start = T - N_FOLDS * FOLD_PERIODS   # index where validation window begins

    if val_start <= SEQ_LEN:
        log.error("Not enough training data before validation window")
        sys.exit(1)

    val_cutoff_ts = df.timestamp.iloc[val_start + SEQ_LEN]
    log.info(
        "Walk-forward: %d folds × ~%d periods (~6 months each) | val start: %s",
        N_FOLDS, FOLD_PERIODS, val_cutoff_ts.date(),
    )

    # ── Walk-forward validation ───────────────────────────────────────────────
    fold_metrics: list[tuple[float, float]] = []

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
        rmse, mae = evaluate(model, fold_X, fold_y)
        fold_metrics.append((rmse, mae))
        log.info("  Fold %d  RMSE=%.4f  MAE=%.4f", fold + 1, rmse, mae)

    mean_rmse = np.mean([m[0] for m in fold_metrics])
    mean_mae  = np.mean([m[1] for m in fold_metrics])
    log.info("Walk-forward summary  mean RMSE=%.4f  mean MAE=%.4f", mean_rmse, mean_mae)

    # ── Final model: train on all data ────────────────────────────────────────
    log.info("Training final model on all %d rows", T)
    all_X, all_y = make_sequences(values, targets)
    split = int(len(all_X) * 0.95)
    final_model = train_model(all_X[:split], all_y[:split], all_X[split:], all_y[split:])

    final_rmse, final_mae = evaluate(final_model, all_X[split:], all_y[split:])
    log.info("Final model hold-out  RMSE=%.4f  MAE=%.4f", final_rmse, final_mae)

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
            },
            "validation": {
                "n_folds": N_FOLDS,
                "fold_periods": FOLD_PERIODS,
                "per_fold": [{"rmse": r, "mae": m} for r, m in fold_metrics],
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
