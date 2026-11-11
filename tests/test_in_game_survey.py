"""The in-game survey driver's parsing and sample, driven by synthetic outputs.

These tests do not start a game. They pin what the driver reports for a run: the last JSON line
of a control reply, one row per jar with its verdict and counted work, the registration totals
the log carries and the report does not, and the sample's shape (40 distinct jars, ten batches).
"""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SURVEY = ROOT / "tools" / "in_game_survey.py"


def load_survey():
    spec = importlib.util.spec_from_file_location("horizon_in_game_survey", SURVEY)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ParseReplyTest(unittest.TestCase):
    def setUp(self):
        self.survey = load_survey()

    def test_last_json_line_wins_over_earlier_ones(self):
        text = '{"ok": false}\nnoise\n{"ok": true, "chat": ["hi"]}\n'
        self.assertEqual(self.survey.parse_reply(text), {"ok": True, "chat": ["hi"]})

    def test_no_json_line_is_none(self):
        self.assertIsNone(self.survey.parse_reply("cannot connect\n"))
        self.assertIsNone(self.survey.parse_reply(""))


class SummariseTest(unittest.TestCase):
    def setUp(self):
        self.survey = load_survey()
        self.report = {
            "schema": 1,
            "jars": [
                {
                    "file": "fancymenu.jar",
                    "id": "fancymenu",
                    "verdict": "ran-clean",
                    "host_calls": 245,
                    "unknown_host_call_count": 0,
                    "reason": None,
                    "callbacks": {"ran": 3, "failed": 0, "unknown_host_call_count": 0},
                    "commands": {"ran": 0, "refused": 1, "unknown_host_call_count": 1},
                    "content": {"blocks": 0, "items": 0, "skipped": []},
                },
                {
                    "file": "broken.jar",
                    "id": "broken",
                    "verdict": "refused",
                    "host_calls": 0,
                    "unknown_host_call_count": 0,
                    "reason": "no main entrypoint",
                    "callbacks": {"ran": 0, "failed": 0, "unknown_host_call_count": 0},
                    "commands": {"ran": 0, "refused": 0, "unknown_host_call_count": 0},
                    "content": {"blocks": 0, "items": 0, "skipped": []},
                },
            ],
        }
        self.log = [
            "Horizon Java: fancymenu.jar (fancymenu) ran-clean (245 host calls, 0 unknown host calls)",
            "Horizon Java: fancymenu: command registration: 1 callback(s) ran, 0 failed",
            "Horizon Java: fancymenu: 7 command usage(s) translated, 9 refused",
            "Horizon Java: fancymenu: /fmvariable get <variable_name:word> refused: unknown native host call ServerPlayer#level()",
            "Horizon Java: server_started (dedicated=false): 3 callback(s) ran, 0 failed, 0 unknown host call(s)",
        ]

    def test_rows_carry_report_totals_and_log_only_totals(self):
        rows = {row["id"]: row for row in self.survey.summarise(self.report, self.log)}
        fancy = rows["fancymenu"]
        self.assertEqual(fancy["verdict"], "ran-clean")
        self.assertEqual(fancy["callbacks"]["ran"], 3)
        self.assertEqual(fancy["usages"], {"translated": 7, "refused": 9})
        self.assertEqual(fancy["registration"], {"ran": 1, "failed": 0})
        self.assertEqual(len(fancy["log_reasons"]), 1)
        self.assertIn("ServerPlayer#level()", fancy["log_reasons"][0])
        # A global hook line is not attributed to a jar.
        self.assertNotIn("server_started", " ".join(fancy["log_reasons"]))

    def test_refused_jar_keeps_its_reason(self):
        rows = {row["id"]: row for row in self.survey.summarise(self.report, self.log)}
        self.assertEqual(rows["broken"]["verdict"], "refused")
        self.assertEqual(rows["broken"]["reason"], "no main entrypoint")
        self.assertEqual(rows["broken"]["verdict_line"], None)

    def test_listener_failures_show_in_the_summary_and_the_table(self):
        # The verdict stays the entrypoint's; the summary is what keeps a jar whose listeners
        # all threw from reading as a clean run.
        self.report["jars"][0]["verdict"] = "ran-clean"
        self.report["jars"][0]["callbacks"] = {
            "ran": 1, "failed": 12, "unknown_host_call_count": 0,
        }
        rows = {row["id"]: row for row in self.survey.summarise(self.report, self.log)}
        self.assertEqual(rows["fancymenu"]["verdict"], "ran-clean")
        self.assertEqual(rows["fancymenu"]["summary"], "ran-clean, 12 listener failures")
        rendered = self.survey.table(self.survey.summarise(self.report, self.log))
        self.assertIn("ran-clean, 12 listener failures", rendered)
        self.assertIn("| `fancymenu.jar` | ran-clean |", rendered)

    def test_the_report_summary_is_preferred_and_singular_counts_read_right(self):
        self.report["jars"][0]["summary"] = "ran-partial, 1 listener failure"
        self.report["jars"][0]["callbacks"]["failed"] = 1
        rows = {row["id"]: row for row in self.survey.summarise(self.report, self.log)}
        self.assertEqual(rows["fancymenu"]["summary"], "ran-partial, 1 listener failure")
        self.report["jars"][0].pop("summary")
        rows = {row["id"]: row for row in self.survey.summarise(self.report, self.log)}
        self.assertEqual(rows["fancymenu"]["summary"], "ran-clean, 1 listener failure")

    def test_registration_totals_land_in_the_usage_columns(self):
        # The registration log line now carries both callback and usage totals; the driver reads
        # them from that one line and the report's commands hold the same counts.
        line = ("Horizon Java: fancymenu: command registration: 0 callback(s) ran, 1 failed, "
                "2 unknown host call(s); 3 command usage(s) translated, 4 refused, "
                "5 unknown host call(s)")
        row = self.survey.summarise(self.report, [line])[0]
        self.assertEqual(row["registration"], {"ran": 0, "failed": 1})
        self.assertEqual(row["usages"], {"translated": 3, "refused": 4})

    def test_missing_report_yields_no_rows(self):
        self.assertEqual(self.survey.summarise(None, ["Horizon Java: x"]), [])


class SampleTest(unittest.TestCase):
    def setUp(self):
        self.survey = load_survey()

    def test_sample_is_ten_batches_of_four_distinct_jars(self):
        batches = self.survey.SAMPLE
        self.assertEqual(len(batches), 10)
        names = [jar[1] for _, jars in batches for jar in jars]
        self.assertEqual(len(names), 40)
        self.assertEqual(len(set(names)), 40)
        for _, jars in batches:
            self.assertEqual(len(jars), 4)

    def test_find_jar_searches_each_corpus_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "mega").mkdir()
            (root / "mega" / "a.jar").write_bytes(b"jar")
            self.assertEqual(self.survey.find_jar(root, "a.jar"), root / "mega" / "a.jar")
            with self.assertRaises(SystemExit):
                self.survey.find_jar(root, "missing.jar")


class ManifestTest(unittest.TestCase):
    def test_manifest_rows_become_name_keyed_jars(self):
        survey = load_survey()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "batches.json"
            path.write_text(json.dumps([
                {"batch": "b1", "jars": [{"file": "a.jar", "command": "hello"},
                                          {"file": "b.jar"}]}
            ]))
            batches = survey.manifest(path)
            self.assertEqual(batches[0][0], "b1")
            self.assertEqual(batches[0][1][0], ("a.jar", "a.jar", "hello"))
            self.assertIsNone(batches[0][1][1][2])


if __name__ == "__main__":
    unittest.main()
