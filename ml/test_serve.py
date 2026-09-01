"""Guards for the audit findings that live in serve.py.

Run from the repository root:

    python -m unittest discover -s ml -p "test_*.py"

Written against `unittest` from the standard library rather than pytest, and
that is the whole point: this adds no dependency to `requirements.txt`, no
second dev requirements file, and nothing to install in CI or on a laptop. The
ml image is already 1.77 GB and a test runner has no business in it. The test
files themselves are excluded from the build by `ml/.dockerignore`, so the image
does not grow by a single byte either. What these tests do need, torch and the
bundled checkpoint, is what serve.py needs anyway.

There were no Python tests at all before 2026-08-31, while three audit findings
live in this directory.
"""

import unittest

import torch

import serve


def _builtins_only(value, path="ckpt"):
    """Every leaf under a checkpoint key must be a builtin, or the restricted
    unpickler will refuse the file at startup instead of at training time."""
    if isinstance(value, dict):
        for k, v in value.items():
            yield from _builtins_only(v, f"{path}[{k!r}]")
    elif isinstance(value, (list, tuple)):
        for i, v in enumerate(value):
            yield from _builtins_only(v, f"{path}[{i}]")
    elif not isinstance(value, (int, float, str, bool, type(None))):
        yield path, type(value).__name__


class CheckpointLoading(unittest.TestCase):
    """AUD-010: the checkpoint was loaded with the unrestricted pickle
    unpickler, so anything that could write to the model volume could execute
    code in the ml container at startup."""

    def test_the_bundled_checkpoint_loads_under_the_restricted_unpickler(self):
        ckpt = torch.load(serve.MODEL_PATH, map_location="cpu", weights_only=True)
        self.assertIn("model_state", ckpt)
        self.assertIn("hyperparams", ckpt)

    def test_the_load_call_in_serve_asks_for_the_restricted_unpickler(self):
        """The test above proves the file *can* load safely. This proves the
        file *is* loaded safely, which is the finding.

        Asserted against the source rather than by loading, because a test that
        passes `weights_only=True` itself would go on passing after the call
        site went back to the permissive loader, which is the shape of guard
        this exercise keeps finding.
        """
        import ast
        import pathlib

        source = pathlib.Path(serve.__file__).read_text(encoding="utf-8")
        loads = [
            node
            for node in ast.walk(ast.parse(source))
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and node.func.attr == "load"
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "torch"
        ]
        self.assertTrue(loads, "serve.py no longer calls torch.load anywhere")
        for call in loads:
            kwargs = {kw.arg: kw.value for kw in call.keywords}
            self.assertIn(
                "weights_only", kwargs,
                f"torch.load at line {call.lineno} does not pass weights_only",
            )
            self.assertEqual(
                getattr(kwargs["weights_only"], "value", None), True,
                f"torch.load at line {call.lineno} does not pass weights_only=True",
            )

    def test_the_saved_metadata_is_builtins_only(self):
        # The comment in serve.py states this contract and nothing enforced it.
        # A metric saved as a numpy scalar or a tensor loads fine today under
        # the old loader and fails at container start under the new one, which
        # is a deploy-time failure a training run cannot see.
        ckpt = torch.load(serve.MODEL_PATH, map_location="cpu", weights_only=True)
        offenders = []
        for key in ("hyperparams", "validation", "trained_through"):
            offenders += list(_builtins_only(ckpt[key], key))
        self.assertEqual(
            offenders,
            [],
            f"non-builtin values in checkpoint metadata: {offenders}",
        )


class PaddingContract(unittest.TestCase):
    """AUD-015: Kp was padded with 0.0, which is a real Kp meaning very quiet
    rather than a neutral marker, so most of every production window was
    fabricated and biased toward calm."""

    @classmethod
    def setUpClass(cls):
        ckpt = torch.load(serve.MODEL_PATH, map_location="cpu", weights_only=True)
        cls.hp = ckpt["hyperparams"]
        cls.seq_len = cls.hp["seq_len"]
        cls.features = cls.hp["features"]
        cls.kp_scaled = set(cls.hp["kp_scaled_features"])
        # Deliberately never 0.0 and never equal to each other, so a fabricated
        # cell cannot hide behind a supplied value.
        cls.readings = [1.0 + 0.25 * i for i in range(cls.seq_len)]

    def test_a_short_window_is_refused_rather_than_padded(self):
        with self.assertRaises(ValueError):
            serve.build_sequence(self.readings[:-1], None, None, self.hp)

    def test_a_full_window_carries_the_supplied_readings_untouched(self):
        x = serve.build_sequence(self.readings, None, None, self.hp)
        self.assertEqual(tuple(x.shape), (1, self.seq_len, self.hp["n_features"]))
        col = self.features.index("kp")
        kp_max = self.hp["kp_max"]
        for j, supplied in enumerate(self.readings):
            self.assertAlmostEqual(
                float(x[0, j, col]), supplied / kp_max, places=6,
                msg=f"slot {j} of the kp column is not the value the caller supplied",
            )

    def test_the_remaining_fabricated_cells_are_exactly_the_known_thirty(self):
        """The part of AUD-015 that is still open, pinned so it cannot grow.

        Lags and rolling windows are computed inside the supplied window, so
        `lag_k` is 0.0 for the first k slots and the two rolling features are
        0.0 at slot 0: 1+2+3+4+5+6+7 = 28, plus 2, out of 16 by 19 = 304 cells.
        Closing it means asking for seq_len + 7 readings and computing the lags
        across the overhang. Until then this test fails if the count moves in
        either direction, so neither a regression nor the fix lands unnoticed.
        """
        x = serve.build_sequence(self.readings, None, None, self.hp)
        fabricated = [
            (j, self.features[c])
            for j in range(self.seq_len)
            for c in range(len(self.features))
            if self.features[c] in self.kp_scaled and float(x[0, j, c]) == 0.0
        ]
        self.assertEqual(
            len(fabricated), 30,
            f"expected the 28 lag cells plus 2 rolling cells, got {fabricated}",
        )
        self.assertTrue(
            all(name != "kp" for _, name in fabricated),
            "the kp column itself must never be fabricated",
        )


class PublishedInterval(unittest.TestCase):
    """AUD-014: the band is labelled a 95 percent confidence interval in six
    files and is MC Dropout spread with no observation noise term.

    Measured on 2026-08-31 against 1229 forecasts paired with the observed
    three-hour Kp: **13.1 percent** of observations fell inside the interval,
    mean width 0.405 Kp against a mean absolute error of 0.727 Kp. The error is
    larger than the whole interval.

    No unit test can assert 95 percent coverage, because coverage is a property
    of the model and the data rather than of this file, and asserting it would
    fail today. What is testable is the construction, so a change to it is
    deliberate rather than incidental, and the finding stays open until the
    interval carries an observation noise term and is recalibrated.
    """

    def test_the_interval_is_built_from_the_documented_constants(self):
        self.assertEqual(serve.CI_Z, 1.96)
        self.assertEqual(serve.MC_SAMPLES, 50)


if __name__ == "__main__":
    unittest.main()
