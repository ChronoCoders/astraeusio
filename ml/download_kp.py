"""
Download 20 years of Kp index data from GFZ Niemegk (the authoritative source,
referenced by NOAA). The dataset covers 1932–present in a single file.
We stream it, split by year, and save one file per year to data/kp_raw/.

Column layout (space-separated, comment lines start with #):
  YYYY MM DD  DOY  DOY+0.5  Bartels_rotation  day_in_rotation
  Kp1..Kp8  (3-hourly, in thirds: 0.333 = 0+, 0.667 = 1-, 1.0 = 1, ...)
  ap1..ap8  Ap  SN  F107  D
"""

import logging
import sys
from datetime import date
from pathlib import Path

import requests

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger(__name__)

SOURCE_URL = "https://www-app3.gfz-potsdam.de/kp_index/Kp_ap_Ap_SN_F107_since_1932.txt"
OUT_DIR = Path(__file__).parent.parent / "data" / "kp_raw"

CURRENT_YEAR = date.today().year
START_YEAR = CURRENT_YEAR - 20  # 20 years of history


def download_raw(url: str) -> list[str]:
    log.info("Fetching %s", url)
    resp = requests.get(url, stream=True, timeout=120)
    resp.raise_for_status()

    total = int(resp.headers.get("content-length", 0))
    chunks: list[bytes] = []
    received = 0

    for chunk in resp.iter_content(chunk_size=65536):
        chunks.append(chunk)
        received += len(chunk)
        if total:
            pct = received / total * 100
            print(f"\r  {received:,} / {total:,} bytes  ({pct:.1f}%)", end="", flush=True)

    print()
    log.info("Download complete — %d bytes", received)
    return b"".join(chunks).decode("utf-8").splitlines()


def split_by_year(lines: list[str]) -> dict[int, list[str]]:
    years: dict[int, list[str]] = {}
    for line in lines:
        if line.startswith("#") or not line.strip():
            continue
        try:
            year = int(line.split()[0])
        except (ValueError, IndexError):
            continue
        if START_YEAR <= year <= CURRENT_YEAR:
            years.setdefault(year, []).append(line)
    return years


def write_years(years: dict[int, list[str]]) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    header = (
        "# YYYY MM DD DOY DOY.5 Bartels_rotation day_in_rotation "
        "Kp1 Kp2 Kp3 Kp4 Kp5 Kp6 Kp7 Kp8 "
        "ap1 ap2 ap3 ap4 ap5 ap6 ap7 ap8 Ap SN F107 D\n"
    )

    for year in sorted(years):
        rows = years[year]
        path = OUT_DIR / f"kp_{year}.txt"
        path.write_text(header + "\n".join(rows) + "\n", encoding="utf-8")
        log.info("Year %d — %d daily records → %s", year, len(rows), path.name)


def main() -> None:
    log.info("Downloading Kp index data for %d–%d", START_YEAR, CURRENT_YEAR)
    try:
        lines = download_raw(SOURCE_URL)
    except requests.RequestException as exc:
        log.error("Download failed: %s", exc)
        sys.exit(1)

    log.info("Splitting by year (keeping %d–%d)", START_YEAR, CURRENT_YEAR)
    years = split_by_year(lines)

    if not years:
        log.error("No data found for the requested range")
        sys.exit(1)

    write_years(years)
    total_rows = sum(len(v) for v in years.values())
    log.info("Done — %d years, %d total daily records saved to %s", len(years), total_rows, OUT_DIR)


if __name__ == "__main__":
    main()
