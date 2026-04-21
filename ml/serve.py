"""
FastAPI inference server for the Kp LSTM model.

POST /predict
  Body: { "readings": [float, ...] }   — 7 to 48 Kp values, oldest first
  Returns predicted next-3h Kp with a 95% confidence interval derived from
  Monte Carlo Dropout (50 stochastic forward passes).

Run: uvicorn ml.serve:app --port 8000
"""

import math
from contextlib import asynccontextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import numpy as np
import torch
import torch.nn as nn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field, field_validator

# ── Constants matching train.py ────────────────────────────────────────────────

MODEL_PATH = Path(__file__).parent / "models" / "kp_lstm.pt"

SEQ_LEN = 16          # loaded from checkpoint; defined here for type clarity
N_FEATURES = 7
MC_SAMPLES = 50       # stochastic forward passes for uncertainty estimate
CI_Z = 1.96           # 95 % confidence interval

# Solar cycle reference — must match preprocess.py exactly
_CYCLE_REF = datetime(2019, 12, 1, tzinfo=timezone.utc)
_CYCLE_PERIOD_DAYS = 4018.5

_THREE_HOURS = timedelta(hours=3)


# ── Model definition (must mirror train.py) ────────────────────────────────────

class KpLSTM(nn.Module):
    def __init__(self, n_features: int, hidden: int, n_layers: int, dropout: float) -> None:
        super().__init__()
        self.lstm = nn.LSTM(
            input_size=n_features,
            hidden_size=hidden,
            num_layers=n_layers,
            batch_first=True,
            dropout=dropout if n_layers > 1 else 0.0,
        )
        self.head = nn.Sequential(
            nn.Linear(hidden, 32),
            nn.ReLU(),
            nn.Linear(32, 1),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        out, _ = self.lstm(x)
        return self.head(out[:, -1, :]).squeeze(1)


# ── Feature engineering ────────────────────────────────────────────────────────

def _sin_cos(value: float, period: float) -> tuple[float, float]:
    angle = 2 * math.pi * value / period
    return math.sin(angle), math.cos(angle)


def _solar_cycle_features(ts: datetime) -> tuple[float, float]:
    elapsed = (ts - _CYCLE_REF).total_seconds() / 86400
    phase = elapsed % _CYCLE_PERIOD_DAYS
    return _sin_cos(phase, _CYCLE_PERIOD_DAYS)


def build_feature_row(kp_norm: float, ts: datetime) -> list[float]:
    """Return one feature vector matching the training schema."""
    hour_sin, hour_cos   = _sin_cos(ts.hour, 24.0)
    month_sin, month_cos = _sin_cos(ts.month - 1, 12.0)
    sc_sin, sc_cos       = _solar_cycle_features(ts)
    return [kp_norm, hour_sin, hour_cos, month_sin, month_cos, sc_sin, sc_cos]


def build_sequence(kp_values: list[float], kp_max: float) -> torch.Tensor:
    """
    Construct a (1, SEQ_LEN, N_FEATURES) tensor from a list of raw Kp values.

    Timestamps are synthesised backwards from now at 3-hour intervals so
    cyclical time features are accurate without requiring the caller to supply
    them.  If fewer than SEQ_LEN values are provided the sequence is
    left-padded with zeros (Kp=0, time features computed for those slots).
    """
    now = datetime.now(timezone.utc).replace(minute=0, second=0, microsecond=0)
    # Snap to the nearest 3-hour boundary
    now = now - timedelta(hours=now.hour % 3)

    n = len(kp_values)
    # Build timestamps for each input reading (oldest first)
    timestamps = [now - _THREE_HOURS * (n - 1 - i) for i in range(n)]

    rows = [build_feature_row(kp / kp_max, ts) for kp, ts in zip(kp_values, timestamps)]

    # Left-pad to SEQ_LEN with zero rows if needed
    if len(rows) < SEQ_LEN:
        pad_count = SEQ_LEN - len(rows)
        pad_start = timestamps[0] - _THREE_HOURS * pad_count
        pad_rows = [
            build_feature_row(0.0, pad_start + _THREE_HOURS * i)
            for i in range(pad_count)
        ]
        rows = pad_rows + rows

    # Use only the most recent SEQ_LEN rows if more were given
    rows = rows[-SEQ_LEN:]

    return torch.tensor([rows], dtype=torch.float32)   # (1, SEQ_LEN, N_FEATURES)


# ── App state ─────────────────────────────────────────────────────────────────

class _State:
    model: KpLSTM
    kp_max: float
    meta: dict[str, Any]


state = _State()


@asynccontextmanager
async def lifespan(app: FastAPI):
    if not MODEL_PATH.exists():
        raise RuntimeError(f"Model not found: {MODEL_PATH} — run train.py first")

    ckpt = torch.load(MODEL_PATH, map_location="cpu", weights_only=False)
    hp   = ckpt["hyperparams"]

    state.kp_max = hp["kp_max"]
    state.meta   = {
        "seq_len":        hp["seq_len"],
        "features":       hp["features"],
        "trained_through": ckpt["trained_through"],
        "validation":     ckpt["validation"],
    }

    state.model = KpLSTM(
        n_features=hp["n_features"],
        hidden=hp["hidden"],
        n_layers=hp["n_layers"],
        dropout=hp["dropout"],
    )
    state.model.load_state_dict(ckpt["model_state"])
    state.model.eval()
    yield


app = FastAPI(title="Astraeus Kp Forecast", lifespan=lifespan)


# ── Schema ────────────────────────────────────────────────────────────────────

class PredictRequest(BaseModel):
    readings: list[float] = Field(
        ...,
        min_length=7,
        max_length=48,
        description="Recent Kp values, oldest first, one per 3-hour period (range 0–9).",
    )

    @field_validator("readings")
    @classmethod
    def validate_kp_range(cls, values: list[float]) -> list[float]:
        for v in values:
            if not (0.0 <= v <= 9.0):
                raise ValueError(f"Kp value {v} is out of range [0, 9]")
        return values


class PredictResponse(BaseModel):
    predicted_kp:  float = Field(description="Predicted Kp for the next 3-hour period")
    ci_lower:      float = Field(description="95% confidence interval lower bound")
    ci_upper:      float = Field(description="95% confidence interval upper bound")
    uncertainty:   float = Field(description="1-sigma standard deviation in Kp units")
    horizon_hours: int   = Field(default=3)
    n_mc_samples:  int   = Field(description="MC Dropout forward passes used")
    trained_through: str


# ── Endpoint ──────────────────────────────────────────────────────────────────

@app.post("/predict", response_model=PredictResponse)
async def predict(req: PredictRequest) -> PredictResponse:
    x = build_sequence(req.readings, state.kp_max)   # (1, SEQ_LEN, N_FEATURES)

    # MC Dropout: LSTM internal dropout is gated by training mode, so we must
    # use model.train() to activate it during the stochastic forward passes.
    state.model.train()
    with torch.no_grad():
        samples = torch.stack([state.model(x) for _ in range(MC_SAMPLES)])  # (N, 1)
    state.model.eval()

    samples_kp = samples.squeeze(1).numpy() * state.kp_max   # de-normalise → Kp units

    mean_kp = float(np.mean(samples_kp))
    std_kp  = float(np.std(samples_kp))

    predicted = float(np.clip(mean_kp, 0.0, 9.0))
    ci_lower  = float(np.clip(mean_kp - CI_Z * std_kp, 0.0, 9.0))
    ci_upper  = float(np.clip(mean_kp + CI_Z * std_kp, 0.0, 9.0))

    return PredictResponse(
        predicted_kp=round(predicted, 3),
        ci_lower=round(ci_lower, 3),
        ci_upper=round(ci_upper, 3),
        uncertainty=round(std_kp, 4),
        n_mc_samples=MC_SAMPLES,
        trained_through=state.meta["trained_through"],
    )


@app.get("/health")
async def health() -> dict[str, Any]:
    return {
        "status": "ok",
        "trained_through": state.meta["trained_through"],
        "mean_rmse": round(state.meta["validation"]["mean_rmse"], 4),
        "mean_mae":  round(state.meta["validation"]["mean_mae"], 4),
    }
