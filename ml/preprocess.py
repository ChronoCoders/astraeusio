"""
Preprocess 20 years of Kp index data for LSTM/Transformer training.

Input:  data/kp_raw/kp_YYYY.txt  (one file per year, 8 three-hourly Kp per day)
Output: data/kp_processed.parquet

Schema
------
timestamp       : datetime64[ns, UTC]  — period start (3-hour cadence)
kp              : float32              — Kp index value (0–9, thirds)
hour            : int8                 — 0 6 12 18 ... wait, 3-hourly so 0 3 6 9 12 15 18 21
month           : int8                 — 1–12
day_of_year     : int16                — 1–366
solar_cycle_phase_sin/cos : float32   — ~11-year cycle encoded cyclically
hour_sin/cos    : float32             — hour of day encoded cyclically
month_sin/cos   : float32            — month encoded cyclically
lag_1..lag_7    : float32             — previous 1–7 periods (each = 3 hours)
kp_24h_max      : float32             — rolling max over prior 24 h (8 periods)
kp_72h_mean     : float32            — rolling mean over prior 72 h (24 periods)
gap_filled      : bool                — True if this value was imputed
"""

import logging
import sys
from pathlib import Path

import numpy as np
import pandas as pd

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger(__name__)

RAW_DIR = Path(__file__).parent.parent / "data" / "kp_raw"
OUT_FILE = Path(__file__).parent.parent / "data" / "kp_processed.parquet"

# GFZ column layout (after stripping comment lines)
_RAW_COLS = [
    "year", "month_raw", "day",
    "doy", "doy_mid",
    "bartels", "bartels_day",
    "kp1", "kp2", "kp3", "kp4", "kp5", "kp6", "kp7", "kp8",
    "ap1", "ap2", "ap3", "ap4", "ap5", "ap6", "ap7", "ap8",
    "ap_daily", "sn", "f107_adj", "f107_obs", "d",
]
_KP_COLS = ["kp1", "kp2", "kp3", "kp4", "kp5", "kp6", "kp7", "kp8"]

# Solar cycle 24 minimum (Dec 2019) used as phase reference.
# ~11-year period = 4018.5 days.
_CYCLE_REF = pd.Timestamp("2019-12-01", tz="UTC")
_CYCLE_PERIOD_DAYS = 4018.5

N_LAGS = 7


# ── Parsing ────────────────────────────────────────────────────────────────────

def load_raw_year(path: Path) -> pd.DataFrame:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        parts = line.split()
        if len(parts) < len(_RAW_COLS):
            continue
        rows.append(parts[: len(_RAW_COLS)])
    df = pd.DataFrame(rows, columns=_RAW_COLS)
    for col in ["year", "month_raw", "day"]:
        df[col] = df[col].astype(int)
    for col in _KP_COLS:
        df[col] = pd.to_numeric(df[col], errors="coerce")
    return df


def to_3hourly(df: pd.DataFrame) -> pd.DataFrame:
    """Expand daily rows (8 Kp readings) to one row per 3-hour period."""
    records = []
    hours = [0, 3, 6, 9, 12, 15, 18, 21]
    for _, row in df.iterrows():
        base = pd.Timestamp(
            year=int(row["year"]),
            month=int(row["month_raw"]),
            day=int(row["day"]),
            tz="UTC",
        )
        for i, h in enumerate(hours):
            kp_val = row[_KP_COLS[i]]
            records.append({"timestamp": base + pd.Timedelta(hours=h), "kp": kp_val})
    return pd.DataFrame(records)


# ── Gap handling ───────────────────────────────────────────────────────────────

def fill_gaps(df: pd.DataFrame) -> pd.DataFrame:
    """
    Reindex to a complete 3-hour grid. Flag imputed values.
    Short gaps (≤ 3 missing periods = 9 h): linear interpolation.
    Longer gaps: forward-fill then backward-fill (last resort).
    """
    full_idx = pd.date_range(df["timestamp"].min(), df["timestamp"].max(), freq="3h", tz="UTC")
    df = df.set_index("timestamp").reindex(full_idx)
    df.index.name = "timestamp"

    missing_mask = df["kp"].isna()
    df["gap_filled"] = missing_mask

    # Interpolate short gaps (≤ 3 consecutive NaNs)
    df["kp"] = df["kp"].interpolate(method="linear", limit=3)
    # Fill remaining with forward/backward fill
    df["kp"] = df["kp"].ffill().bfill()

    n_filled = missing_mask.sum()
    if n_filled:
        log.warning("Imputed %d missing 3-hour periods", n_filled)

    return df.reset_index()


# ── Features ───────────────────────────────────────────────────────────────────

def _sin_cos(series: pd.Series, period: float) -> tuple[pd.Series, pd.Series]:
    angle = 2 * np.pi * series / period
    return np.sin(angle).astype("float32"), np.cos(angle).astype("float32")


def build_features(df: pd.DataFrame) -> pd.DataFrame:
    df = df.copy()
    df["kp"] = df["kp"].astype("float32")

    # Lag features (each step = 3 hours)
    for lag in range(1, N_LAGS + 1):
        df[f"lag_{lag}"] = df["kp"].shift(lag).astype("float32")

    # Rolling statistics (computed on past data only, hence shift(1) window)
    df["kp_24h_max"] = df["kp"].shift(1).rolling(8).max().astype("float32")   # 8×3h = 24h
    df["kp_72h_mean"] = df["kp"].shift(1).rolling(24).mean().astype("float32")  # 24×3h = 72h

    # Time components
    ts = df["timestamp"]
    df["hour"] = ts.dt.hour.astype("int8")
    df["month"] = ts.dt.month.astype("int8")
    df["day_of_year"] = ts.dt.day_of_year.astype("int16")

    # Cyclical encodings
    df["hour_sin"], df["hour_cos"] = _sin_cos(ts.dt.hour.astype(float), 24.0)
    df["month_sin"], df["month_cos"] = _sin_cos(ts.dt.month.astype(float) - 1, 12.0)

    # Solar cycle phase: days elapsed since reference minimum, mod period
    elapsed_days = (ts - _CYCLE_REF).dt.total_seconds() / 86400
    df["solar_cycle_phase_sin"], df["solar_cycle_phase_cos"] = _sin_cos(
        elapsed_days % _CYCLE_PERIOD_DAYS, _CYCLE_PERIOD_DAYS
    )

    # Drop leading rows that lack enough history for all lags + rolling windows
    min_history = max(N_LAGS, 24)  # 24 periods for 72h mean
    df = df.iloc[min_history:].reset_index(drop=True)

    return df


# ── Main ───────────────────────────────────────────────────────────────────────

def main() -> None:
    raw_files = sorted(RAW_DIR.glob("kp_*.txt"))
    if not raw_files:
        log.error("No files found in %s — run download_kp.py first", RAW_DIR)
        sys.exit(1)

    log.info("Loading %d year files from %s", len(raw_files), RAW_DIR)

    yearly: list[pd.DataFrame] = []
    for path in raw_files:
        year = path.stem.split("_")[1]
        raw = load_raw_year(path)
        expanded = to_3hourly(raw)
        log.info("Year %s — %d daily rows → %d 3-hour periods", year, len(raw), len(expanded))
        yearly.append(expanded)

    combined = pd.concat(yearly, ignore_index=True).sort_values("timestamp")

    log.info("Total periods before gap fill: %d", len(combined))
    filled = fill_gaps(combined)
    log.info("Total periods after gap fill:  %d", len(filled))

    log.info("Building features (lags 1–%d, rolling windows, cyclical encodings)", N_LAGS)
    processed = build_features(filled)

    n_gaps = processed["gap_filled"].sum()
    log.info(
        "Final dataset: %d rows, %d columns, %d gap-filled values",
        len(processed), len(processed.columns), n_gaps,
    )

    OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    processed.to_parquet(OUT_FILE, index=False)
    log.info("Saved → %s  (%.1f MB)", OUT_FILE, OUT_FILE.stat().st_size / 1e6)


if __name__ == "__main__":
    main()
