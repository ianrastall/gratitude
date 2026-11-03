import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from compile_db import ROOT, compile_db, place, seconds


class CompileDbTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.path = Path(cls.temp.name) / "berthaml.db"
        compile_db(cls.path)
        cls.db = sqlite3.connect(f"file:{cls.path}?mode=ro", uri=True)

    @classmethod
    def tearDownClass(cls):
        cls.db.close()
        cls.temp.cleanup()

    def count(self, sql):
        return self.db.execute(sql).fetchone()[0]

    def test_counts_match_the_coverage_report(self):
        coverage = json.loads((ROOT / "reports/coverage.json").read_text(encoding="utf-8"))
        self.assertEqual(self.count("PRAGMA user_version"), 2)
        for table in ("shows", "songs", "venues", "people", "recordings"):
            self.assertEqual(self.count(f"SELECT count(*) FROM {table}"), coverage[table], table)
        self.assertEqual(self.count("SELECT count(*) FROM performances WHERE song IS NOT NULL"), coverage["performances"])
        self.assertEqual(self.db.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_show_facts_survive_compilation(self):
        rows = self.db.execute(
            "SELECT g.title, p.transition FROM performances p JOIN songs g ON g.id = p.song "
            "WHERE p.show = 'gd1977-12-27' ORDER BY p.set_n, p.n"
        ).fetchall()
        self.assertEqual(rows[0], ("Bertha", "segue"))
        self.assertEqual(len(rows), 23)
        self.assertEqual(self.count("SELECT count(*) FROM lineup WHERE show = 'gd1970-05-02'"), 6)
        level = self.db.execute("SELECT corroboration FROM shows WHERE id = 'gd1977-12-27'").fetchone()[0]
        self.assertEqual(level, "recording-backed")

    def test_tours_group_shows(self):
        tours = json.loads((ROOT / "reports/tours.json").read_text(encoding="utf-8"))
        self.assertEqual(self.count("SELECT count(*) FROM tours"), len(tours))
        self.assertEqual(self.count("SELECT count(*) FROM show_tours"), sum(len(t["shows"]) for t in tours))
        name = self.db.execute("SELECT t.name FROM show_tours s JOIN tours t ON t.id = s.tour WHERE s.show = 'gd1977-05-08'").fetchone()
        self.assertEqual(name, ("Spring 1977",))

    def test_recordings_use_application_codes(self):
        kinds = {k for (k,) in self.db.execute("SELECT DISTINCT kind FROM recordings")}
        self.assertLessEqual(kinds, {"SBD", "MTX", "AUD", "FM", "VIDEO", "UNKNOWN"})
        inferred = self.db.execute("SELECT kind, inferred FROM recordings WHERE id = 'r-2057804e05fca784675b'").fetchone()
        self.assertEqual(inferred, ("SBD", 1))
        self.assertGreater(self.count("SELECT count(*) FROM tracks WHERE seconds > 0"), 0)

    def test_album_places_keep_earlier_spellings(self):
        self.assertEqual(place("NY", "US", "Ithaca"), "NY")
        self.assertEqual(place("", "CA", "Toronto"), "Canada")
        self.assertEqual(place("", "GB", "London"), "England")
        self.assertEqual(place("", "GB", "Edinburgh"), "Scotland")
        self.assertEqual(seconds("PT1H2M3.5S"), 3723.5)
        self.assertIsNone(seconds(None))


if __name__ == "__main__":
    unittest.main()
