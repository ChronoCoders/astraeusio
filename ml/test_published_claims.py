"""The published forecast skill must agree with the code it describes.

`frontend/public/.well-known/agent-skills/get-kp-forecast/SKILL.md` is read by
agents integrating the API, and on 2026-09-23 it stated the wrong training
source, the wrong cadence, the wrong start year, a variable input window against
a fixed one, a feature list missing three solar drivers, and one horizon where
four are served. None of it was caught, because every sweep searched for the
phrases already known to be false rather than for the claims.

These tests read the numbers out of the code and assert the file carries them.
"""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
SKILL = ROOT / "frontend/public/.well-known/agent-skills/get-kp-forecast/SKILL.md"
TRAIN = ROOT / "ml/train.py"
DOWNLOAD = ROOT / "ml/download_kp.py"


def code(path, pattern):
    """One capture group out of a source file, or a failure naming the pattern."""
    m = re.search(pattern, path.read_text(encoding="utf-8"), re.M)
    assert m, "%s does not match %r; the check is reading the wrong thing" % (path.name, pattern)
    return m.group(1)


class PublishedForecastSkill(unittest.TestCase):
    def setUp(self):
        self.text = SKILL.read_text(encoding="utf-8")
        # A floor: an empty or truncated file would pass every absence check
        # below, so the fixture proves it is reading the real document.
        self.assertIn("Get ML Kp Forecast", self.text, "the skill file is not what it should be")
        self.assertGreater(len(self.text), 1500, "the skill file is too short to be intact")

    def test_the_stated_input_window_is_the_one_the_model_uses(self):
        seq_len = code(TRAIN, r"^SEQ_LEN\s*=\s*(\d+)")
        self.assertIn(
            f"{seq_len} readings", self.text,
            f"the skill must state the fixed {seq_len}-reading window from train.py",
        )
        # The variable range this replaced, which came from a defect.
        for stale in ("7-48", "7–48", "7 to 48"):
            self.assertNotIn(stale, self.text, "the variable window claim is back")

    def test_every_horizon_the_model_serves_is_stated(self):
        horizons = re.findall(r"\d+", code(TRAIN, r"^HORIZON_HOURS\s*=\s*\[([^\]]+)\]"))
        self.assertEqual(horizons, ["3", "6", "12", "24"], "horizons moved; update this test too")
        for h in horizons:
            self.assertRegex(
                self.text, rf"\b{h}\b",
                f"the skill does not mention the {h} hour horizon",
            )
        self.assertNotIn(
            "3-hour ahead", self.text,
            "the skill still describes a single 3 hour forecast",
        )

    def test_the_training_source_is_the_one_the_downloader_uses(self):
        url = code(DOWNLOAD, r'^SOURCE_URL\s*=\s*"([^"]+)"')
        self.assertIn("gfz", url.lower(), "the downloader no longer pulls from GFZ")
        self.assertIn("GFZ", self.text, "the skill must name GFZ as the training source")
        self.assertNotIn(
            "NOAA 1-minute estimated Kp", self.text,
            "the skill still claims NOAA one-minute Kp as training data",
        )

    def test_the_solar_drivers_are_not_omitted_from_the_feature_list(self):
        minmax = code(TRAIN, r"^MINMAX_FEATURES\s*=\s*\[([^\]]+)\]")
        self.assertIn("f107_adj", minmax)
        for phrase in ("F10.7", "sunspot"):
            self.assertIn(
                phrase, self.text,
                f"the feature list omits {phrase}, which the model is fed",
            )


if __name__ == "__main__":
    unittest.main()
