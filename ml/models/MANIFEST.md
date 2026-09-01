# Deployed models

The checkpoint itself is not in version control: `.gitignore` excludes
`ml/models/*` because a 230 KB binary does not belong in a diff. What belongs in
version control is its identity and its numbers, which is this file. One row per
model that has been deployed, newest first.

`ml /health` reports `model_sha256` for whatever is actually loaded, so a row
here can always be matched against what is running. `./deploy-model.sh --list`
shows what is on the volume.

Every MAE below is measured **at matched leads**: model, persistence and the
two-parameter linear fit all answering the same horizon. Anything measured
before 2026-09-01 was not, because of AUD-032, and is not comparable.

## 061a5d30fac5

- **sha256** `061a5d30fac50c5f7e941730a37726c2bf02c008f72f484e8c01f143274760d1`
- **trained** 2026-08-31, repo at `b02d4bc`
- **trained through** 2026-04-20
- **target** residual at 3h and 6h, level at 12h and 24h
- **first with correct horizon labels.** Every earlier model was trained one
  period beyond its label (AUD-032), so its stored metrics describe 6h/9h/15h/27h.

Walk-forward MAE, 4 folds, 2024-04-21 to 2026-04-20, 5840 slots, 299 at Kp >= 5:

| Horizon | Model | Two-param fit | Persistence |
|---|---|---|---|
| 3h | **0.655** | 0.669 | 0.684 |
| 6h | **0.799** | 0.826 | 0.882 |
| 12h | **0.928** | 0.958 | 1.061 |
| 24h | **1.026** | 1.054 | 1.228 |

Out-of-sample, 375 held-out windows, 2026-07-14 to 2026-08-29, quiet:

| Horizon | Model | Two-param fit | Persistence |
|---|---|---|---|
| 3h | 0.577 | **0.568** | 0.581 |
| 6h | **0.660** | 0.670 | 0.696 |
| 12h | 0.802 | **0.797** | 0.857 |
| 24h | **0.874** | 0.923 | 1.009 |

Known limitations, tracked: the four heads are one prediction with four
dampings (AUD-033), the model still loses to persistence at Kp >= 5 (AUD-031),
and the published band is not a calibrated interval (AUD-014).

## Before this

No record exists. Models were copied onto the volume by hand and nothing
recorded which run produced them. The checkpoint that served until this one is
preserved on the volume by hash and in the 2026-09-01 session scratchpad.
