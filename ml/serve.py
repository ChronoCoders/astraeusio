"""
FastAPI inference server for the multi-horizon Kp LSTM model.

POST /predict
  Body: { "readings": [float, ...] }            - 7 to 48 Kp values, oldest first
         optional "f107": [float, ...]          - F10.7 adjusted flux (sfu), same length
         optional "sunspot": [float, ...]        - daily sunspot number, same length
  Returns predicted Kp at 3h/6h/12h/24h, each with a 95% confidence interval
  derived from Monte Carlo Dropout (50 stochastic forward passes). The top-level
  fields (predicted_kp / ci_lower / ci_upper / uncertainty) mirror the 3-hour
  horizon for backward compatibility.

Run: uvicorn ml.serve:app --port 8000
"""

import math
import os
from contextlib import asynccontextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import numpy as np
import torch
import torch.nn as nn
from fastapi import FastAPI
from fastapi.openapi.docs import get_redoc_html, get_swagger_ui_html
from pydantic import BaseModel, Field, field_validator, model_validator

# ── Constants ────────────────────────────────────────────────────────────────

MODEL_PATH = Path(os.getenv("MODEL_PATH", str(Path(__file__).parent / "models" / "kp_lstm.pt")))

MC_SAMPLES = 50       # stochastic forward passes for uncertainty estimate
CI_Z = 1.96           # 95 % confidence interval

# Solar cycle reference - must match preprocess.py exactly
_CYCLE_REF = datetime(2019, 12, 1, tzinfo=timezone.utc)
_CYCLE_PERIOD_DAYS = 4018.5

_THREE_HOURS = timedelta(hours=3)
_F107_DELTA_PERIODS = 8   # 24 h, must match preprocess.py

KP_VALID_MAX = 9.0


# ── Model definition (must mirror train.py) ────────────────────────────────────

class KpLSTM(nn.Module):
    def __init__(self, n_features: int, hidden: int, n_layers: int, dropout: float, n_horizons: int) -> None:
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
            nn.Linear(32, n_horizons),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        out, _ = self.lstm(x)
        return self.head(out[:, -1, :])   # (B, n_horizons)


# ── Feature engineering ────────────────────────────────────────────────────────

def _sin_cos(value: float, period: float) -> tuple[float, float]:
    angle = 2 * math.pi * value / period
    return math.sin(angle), math.cos(angle)


def _solar_cycle_features(ts: datetime) -> tuple[float, float]:
    phase = ((ts - _CYCLE_REF).total_seconds() / 86400) % _CYCLE_PERIOD_DAYS
    return _sin_cos(phase, _CYCLE_PERIOD_DAYS)


def _scale_minmax(value: float, lo: float, hi: float) -> float:
    return (min(max(value, lo), hi) - lo) / (hi - lo)


def build_sequence(
    kp_values: list[float],
    f107_values: list[float] | None,
    sunspot_values: list[float] | None,
    hp: dict[str, Any],
) -> torch.Tensor:
    """
    Construct a (1, SEQ_LEN, N_FEATURES) tensor matching the training schema.

    Timestamps are synthesised backwards from now at 3-hour intervals. Kp, F10.7
    and sunspot inputs are aligned and left-padded to SEQ_LEN; missing optional
    inputs fall back to the per-feature training-mean defaults from the
    checkpoint (graceful degradation - Kp-only callers work unchanged).
    All normalisation constants come from the checkpoint, never hardcoded here.
    """
    seq_len: int = hp["seq_len"]
    features: list[str] = hp["features"]
    kp_max: float = hp["kp_max"]
    kp_scaled: set[str] = set(hp["kp_scaled_features"])
    minmax: dict[str, list[float]] = hp["minmax"]
    defaults: dict[str, float] = hp["feature_defaults"]

    now = datetime.now(timezone.utc).replace(minute=0, second=0, microsecond=0)
    now = now - timedelta(hours=now.hour % 3)   # snap to a 3-hour boundary

    n = len(kp_values)
    pad = max(0, seq_len - n)

    # Build raw per-slot series, left-padded to seq_len (oldest first).
    f107_default = defaults["f107_adj"]
    sn_default = defaults["sn"]

    kp_seq = [0.0] * pad + list(kp_values)
    f107_seq = ([f107_default] * pad + list(f107_values)) if f107_values else [f107_default] * (pad + n)
    sn_seq = ([sn_default] * pad + list(sunspot_values)) if sunspot_values else [sn_default] * (pad + n)

    # Keep only the most recent seq_len slots if more were supplied.
    kp_seq, f107_seq, sn_seq = kp_seq[-seq_len:], f107_seq[-seq_len:], sn_seq[-seq_len:]
    timestamps = [now - _THREE_HOURS * (seq_len - 1 - i) for i in range(seq_len)]

    rows: list[list[float]] = []
    for j in range(seq_len):
        ts = timestamps[j]
        hour_sin, hour_cos = _sin_cos(ts.hour, 24.0)
        month_sin, month_cos = _sin_cos(ts.month - 1, 12.0)
        sc_sin, sc_cos = _solar_cycle_features(ts)

        prior = kp_seq[max(0, j - 8):j]            # shift(1).rolling(8) window
        prior72 = kp_seq[max(0, j - 24):j]         # shift(1).rolling(24) window
        delta = f107_seq[j] - f107_seq[j - _F107_DELTA_PERIODS] if j >= _F107_DELTA_PERIODS else 0.0

        raw = {
            "kp": kp_seq[j],
            "lag_1": kp_seq[j - 1] if j >= 1 else 0.0,
            "lag_2": kp_seq[j - 2] if j >= 2 else 0.0,
            "lag_3": kp_seq[j - 3] if j >= 3 else 0.0,
            "lag_4": kp_seq[j - 4] if j >= 4 else 0.0,
            "lag_5": kp_seq[j - 5] if j >= 5 else 0.0,
            "lag_6": kp_seq[j - 6] if j >= 6 else 0.0,
            "lag_7": kp_seq[j - 7] if j >= 7 else 0.0,
            "kp_24h_max": max(prior) if prior else 0.0,
            "kp_72h_mean": (sum(prior72) / len(prior72)) if prior72 else 0.0,
            "hour_sin": hour_sin, "hour_cos": hour_cos,
            "month_sin": month_sin, "month_cos": month_cos,
            "solar_cycle_phase_sin": sc_sin, "solar_cycle_phase_cos": sc_cos,
            "f107_adj": f107_seq[j],
            "sn": sn_seq[j],
            "f107_1d_delta": delta,
        }

        row: list[float] = []
        for feat in features:
            v = raw[feat]
            if feat in kp_scaled:
                row.append(v / kp_max)
            elif feat in minmax:
                lo, hi = minmax[feat]
                row.append(_scale_minmax(v, lo, hi))
            else:  # time features, already bounded
                row.append(v)
        rows.append(row)

    return torch.tensor([rows], dtype=torch.float32)   # (1, seq_len, n_features)


# ── App state ─────────────────────────────────────────────────────────────────

class _State:
    model: KpLSTM
    hp: dict[str, Any]
    meta: dict[str, Any]


state = _State()


@asynccontextmanager
async def lifespan(app: FastAPI):
    if not MODEL_PATH.exists():
        raise RuntimeError(f"Model not found: {MODEL_PATH} - run train.py first")

    ckpt = torch.load(MODEL_PATH, map_location="cpu", weights_only=False)
    hp = ckpt["hyperparams"]
    state.hp = hp
    state.meta = {
        "horizons": hp["horizons"],
        "trained_through": ckpt["trained_through"],
        "validation": ckpt["validation"],
    }

    state.model = KpLSTM(
        n_features=hp["n_features"],
        hidden=hp["hidden"],
        n_layers=hp["n_layers"],
        dropout=hp["dropout"],
        n_horizons=len(hp["horizons"]),
    )
    state.model.load_state_dict(ckpt["model_state"])
    state.model.eval()
    yield


app = FastAPI(title="Astraeus Kp Forecast", lifespan=lifespan, docs_url=None, redoc_url=None)


@app.get("/docs", include_in_schema=False)
async def swagger_ui():
    return get_swagger_ui_html(
        openapi_url="/openapi.json",
        title="Astraeus Kp Forecast",
        swagger_js_url="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js",
        swagger_css_url="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css",
    )


@app.get("/redoc", include_in_schema=False)
async def redoc():
    return get_redoc_html(
        openapi_url="/openapi.json",
        title="Astraeus Kp Forecast",
        redoc_js_url="https://unpkg.com/redoc@2.1.3/bundles/redoc.standalone.js",
    )


# ── Schema ────────────────────────────────────────────────────────────────────

class PredictRequest(BaseModel):
    readings: list[float] = Field(
        ...,
        min_length=7,
        max_length=48,
        description="Recent Kp values, oldest first, one per 3-hour period (range 0–9).",
    )
    f107: list[float] | None = Field(
        default=None,
        description="Optional F10.7 adjusted flux (sfu), same length/order as readings.",
    )
    sunspot: list[float] | None = Field(
        default=None,
        description="Optional daily sunspot number, same length/order as readings.",
    )

    @field_validator("readings")
    @classmethod
    def validate_kp_range(cls, values: list[float]) -> list[float]:
        for v in values:
            if not (0.0 <= v <= KP_VALID_MAX):
                raise ValueError(f"Kp value {v} is out of range [0, 9]")
        return values

    @model_validator(mode="after")
    def validate_optional_lengths(self) -> "PredictRequest":
        for name, seq in (("f107", self.f107), ("sunspot", self.sunspot)):
            if seq is not None and len(seq) != len(self.readings):
                raise ValueError(f"{name} length {len(seq)} must match readings length {len(self.readings)}")
        return self


class HorizonForecast(BaseModel):
    horizon_hours: int = Field(description="Forecast lead time in hours")
    predicted_kp: float = Field(description="Predicted Kp for this horizon")
    ci_lower: float = Field(description="95% confidence interval lower bound")
    ci_upper: float = Field(description="95% confidence interval upper bound")
    uncertainty: float = Field(description="1-sigma standard deviation in Kp units")


class PredictResponse(BaseModel):
    forecast: list[HorizonForecast] = Field(description="One entry per horizon (3h/6h/12h/24h)")
    n_mc_samples: int = Field(description="MC Dropout forward passes used")
    trained_through: str
    # Flat 3-hour fields mirrored for backward compatibility with existing callers.
    predicted_kp: float = Field(description="Predicted Kp for the next 3-hour period")
    ci_lower: float = Field(description="95% CI lower bound (3h horizon)")
    ci_upper: float = Field(description="95% CI upper bound (3h horizon)")
    uncertainty: float = Field(description="1-sigma std dev in Kp units (3h horizon)")
    horizon_hours: int = Field(default=3)


# ── Endpoint ──────────────────────────────────────────────────────────────────

@app.post("/predict", response_model=PredictResponse)
async def predict(req: PredictRequest) -> PredictResponse:
    x = build_sequence(req.readings, req.f107, req.sunspot, state.hp)  # (1, SEQ_LEN, N_FEATURES)
    kp_max = state.hp["kp_max"]
    horizons = state.meta["horizons"]

    # MC Dropout: LSTM internal dropout is gated by training mode, so we switch
    # to train() to activate it during the stochastic forward passes.
    state.model.train()
    with torch.no_grad():
        samples = torch.stack([state.model(x) for _ in range(MC_SAMPLES)])  # (MC, 1, H)
    state.model.eval()

    samples_kp = samples.squeeze(1).numpy() * kp_max   # (MC, H) de-normalised → Kp units
    mean_kp = np.mean(samples_kp, axis=0)
    std_kp = np.std(samples_kp, axis=0)

    forecast: list[HorizonForecast] = []
    for k, hours in enumerate(horizons):
        m, s = float(mean_kp[k]), float(std_kp[k])
        forecast.append(HorizonForecast(
            horizon_hours=hours,
            predicted_kp=round(float(np.clip(m, 0.0, KP_VALID_MAX)), 3),
            ci_lower=round(float(np.clip(m - CI_Z * s, 0.0, KP_VALID_MAX)), 3),
            ci_upper=round(float(np.clip(m + CI_Z * s, 0.0, KP_VALID_MAX)), 3),
            uncertainty=round(s, 4),
        ))

    head = forecast[0]   # 3-hour horizon
    return PredictResponse(
        forecast=forecast,
        n_mc_samples=MC_SAMPLES,
        trained_through=state.meta["trained_through"],
        predicted_kp=head.predicted_kp,
        ci_lower=head.ci_lower,
        ci_upper=head.ci_upper,
        uncertainty=head.uncertainty,
        horizon_hours=head.horizon_hours,
    )


@app.get("/health")
async def health() -> dict[str, Any]:
    val = state.meta["validation"]
    per_horizon = {
        h: {"rmse": round(m["rmse"], 4), "mae": round(m["mae"], 4)}
        for h, m in val["per_horizon"].items()
    }
    return {
        "status": "ok",
        "trained_through": state.meta["trained_through"],
        "horizons": state.meta["horizons"],
        "per_horizon": per_horizon,
        "mean_rmse": round(val["mean_rmse"], 4),
        "mean_mae": round(val["mean_mae"], 4),
    }
