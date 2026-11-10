import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
_SPEC = importlib.util.spec_from_file_location(
    "unmodelled_calls_tool", ROOT / "tools" / "unmodelled_calls.py"
)
TOOL = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = TOOL
_before = list(sys.path)
_SPEC.loader.exec_module(TOOL)
sys.path[:] = _before


class UnmodelledCallsTests(unittest.TestCase):
    def test_aggregate_counts_jars_calls_and_sole_rows(self):
        corpora = {
            "wide": [
                {"verdict": "ran-hollow", "calls": [["x", 2]], "unknown": 2},
                {"verdict": "ran-hollow", "calls": [["x", 1], ["y", 3]], "unknown": 4},
                {"verdict": "ran-clean", "calls": [], "unknown": 0},
                {"verdict": "refused"},
                {"verdict": "skipped"},
            ],
            "mega": [
                {"verdict": "ran-hollow", "calls": [["y", 5]], "unknown": 5},
            ],
        }
        totals, ranked = TOOL.aggregate(corpora)

        self.assertEqual(totals["wide"], {
            "ran-hollow": 2, "ran-clean": 1, "refused": 1, "skipped": 1,
            "jars": 5, "unknown_total": 6,
        })
        self.assertEqual(totals["mega"]["unknown_total"], 5)
        by_call = {row["call"]: row for row in ranked}
        # x is the only distinct unknown of one jar; y of another. Both are
        # hit by two jars, so total volume breaks the tie.
        self.assertEqual([row["call"] for row in ranked], ["y", "x"])
        self.assertEqual(by_call["x"], {
            "call": "x", "jars": 2, "calls": 3, "sole": 1, "corpora": ["wide"],
        })
        self.assertEqual(by_call["y"], {
            "call": "y", "jars": 2, "calls": 8, "sole": 1,
            "corpora": ["mega", "wide"],
        })

    def test_a_jar_with_two_distinct_calls_is_nobodys_sole_row(self):
        corpora = {"wide": [
            {"verdict": "ran-hollow", "calls": [["a", 4], ["b", 1]], "unknown": 5},
        ]}
        _, ranked = TOOL.aggregate(corpora)
        self.assertEqual([row["sole"] for row in ranked], [0, 0])

    def test_parse_corpus_splits_name_and_directory(self):
        self.assertEqual(TOOL.parse_corpus("neoforge=/tmp/x"),
                         ("neoforge", Path("/tmp/x")))

    def test_locked_neoforge_reads_the_committed_file_names(self):
        locked = TOOL.locked_neoforge()
        self.assertTrue(locked)
        self.assertIn("BiomesOPlenty-neoforge-26.3-26.3.0.0.7.jar", locked)


if __name__ == "__main__":
    unittest.main()
