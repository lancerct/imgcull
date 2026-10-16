import copy
import json
from pathlib import Path
import tempfile
import unittest

import review_ledger as ledger


class ReviewLedgerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir=Path.cwd())
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.root = self.base / "photos"
        self.root.mkdir()
        for name in ("a.NEF", "a.JPG", "b.NEF"):
            (self.root / name).write_bytes(b"unchanged-source")
        self.m = {"root": str(self.root), "photos": [
            {"id": "a", "group": "burst", "files": ["a.NEF", "a.JPG"],
             "protected": False, "existing_rating": None, "existing_label": None},
            {"id": "b", "group": "burst", "files": ["b.NEF"],
             "protected": False, "existing_rating": None, "existing_label": None}]}
        self.db = self.base / "run.db"
        self.manifest = self.write("manifest.json", self.m)
        ledger.init(self.db, self.manifest)

    def write(self, name, obj):
        p = self.base / name
        p.write_text(json.dumps(obj, ensure_ascii=False), encoding="utf-8")
        return p

    def decision(self, ident="a", **kw):
        d = dict(id=ident, rating=3, label=None, reason="distinct good moment",
                 evidence=["full/a.jpg"], full_reviewed=True, native_detail=True,
                 independent_value=True, artistic_score=7, reject_kind=None,
                 alternative_id=None, edit=None)
        d.update(kw)
        return d

    def test_resume_and_export_do_not_touch_sources(self):
        before = {p.name: p.read_bytes() for p in self.root.iterdir()}
        batch = self.write("batch.json", [self.decision()])
        ledger.record(self.db, batch)
        ledger.record(self.db, batch)
        self.assertEqual(ledger.status(self.db)["pending"], ["b"])
        with self.assertRaises(ValueError):
            ledger.export(self.db, self.base / "incomplete.json")
        ledger.record(self.db, self.write("b.json", [self.decision("b", rating=None)]))
        dest = self.base / "plan.json"
        ledger.export(self.db, dest)
        self.assertEqual(ledger.load(dest)["state"], "reviewed_not_applied")
        with self.assertRaises(ValueError):
            ledger.export(self.db, dest)
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.root.iterdir()})

    def test_init_refuses_existing_checkpoint(self):
        with self.assertRaises(FileExistsError):
            ledger.init(self.db, self.manifest)

    def test_duplicate_sidecar_and_escape_blocked(self):
        m = copy.deepcopy(self.m)
        m["photos"][1]["files"] = ["a.JPG"]
        with self.assertRaises(ValueError):
            ledger.validate_manifest(m)
        m["photos"][1]["files"] = ["../manifest.json"]
        with self.assertRaises(ValueError):
            ledger.validate_manifest(m)

    def test_protected_sequence_and_existing_metadata(self):
        for field, value in (("protected", True), ("existing_rating", 5), ("existing_label", "绿色")):
            m = copy.deepcopy(self.m)
            m["photos"][0][field] = value
            d = self.decision(label="green", edit="crop left edge")
            with self.assertRaises(ValueError):
                ledger.validate_decisions(m, {"a": d})

    def test_technical_red_requires_actual_review_flags(self):
        d = self.decision(rating=None, label="red", reject_kind="technical", native_detail=False)
        with self.assertRaises(ValueError):
            ledger.validate_decisions(self.m, {"a": d})

    def test_artistic_red_preserves_record_value(self):
        d = self.decision(rating=None, label="red", reject_kind="artistic", artistic_score=4)
        with self.assertRaises(ValueError):
            ledger.validate_decisions(self.m, {"a": d})

    def test_superseded_requires_retained_alternative_and_revalidates(self):
        d = self.decision(rating=None, label="red", reject_kind="superseded",
                          artistic_score=5, independent_value=False, alternative_id="b")
        with self.assertRaises(ValueError):
            ledger.record(self.db, self.write("invalid.json", [d]))
        ledger.record(self.db, self.write("valid.json", [d, self.decision("b")]))
        with self.assertRaises(ValueError):
            ledger.record(self.db, self.write("changed.json", [self.decision("b", rating=None)]))
        self.assertEqual(ledger.status(self.db)["red"], 1)

    def test_invalid_batch_rolls_back_all(self):
        good = self.decision()
        bad = self.decision("b", rating=4, native_detail=False)
        with self.assertRaises(ValueError):
            ledger.record(self.db, self.write("batch.json", [good, bad]))
        self.assertEqual(ledger.status(self.db)["reviewed"], 0)

    def test_entire_burst_cannot_be_rejected(self):
        ds = {i: self.decision(i, rating=None, label="red", reject_kind="technical") for i in ("a", "b")}
        with self.assertRaises(ValueError):
            ledger.validate_decisions(self.m, ds)

    def test_green_needs_concrete_edit(self):
        with self.assertRaises(ValueError):
            ledger.validate_decisions(self.m, {"a": self.decision(label="green")})

    def test_nonfinite_and_boolean_scores_rejected(self):
        for value in (float("nan"), float("inf"), True):
            with self.assertRaises(ValueError):
                ledger.validate_decisions(self.m, {"a": self.decision(artistic_score=value)})


if __name__ == "__main__":
    unittest.main()
