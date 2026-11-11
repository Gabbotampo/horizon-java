"""The in-game survey driver's parsing and sample, driven by synthetic outputs.

These tests do not start a game. They pin what the driver reports for a run: the last JSON line
of a control reply, one row per jar with its verdict and counted work, the registration totals
the log carries and the report does not, and the sample's shape (40 distinct jars, ten batches).
"""

import importlib.util
import json
import tempfile
import unittest
import zipfile
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

    def test_partial_verdict_keeps_its_failed_stage(self):
        # A stage failed, another ran: the row is ran-partial, never clean, and
        # carries the failed stage's error.
        self.report["jars"] = [{
            "file": "partial.jar",
            "id": "partial",
            "verdict": "ran-partial",
            "host_calls": 2,
            "unknown_host_call_count": 0,
            "reason": "entrypoint(s) failed: demo.Main: Metadata(\"bad marker\")",
            "callbacks": {"ran": 1, "failed": 0, "unknown_host_call_count": 0},
            "commands": {"ran": 0, "refused": 0, "unknown_host_call_count": 0},
            "content": {"blocks": 0, "items": 0, "skipped": []},
        }]
        log = ['Horizon Java: partial.jar (partial) ran-partial (2 host calls, 0 unknown host calls, reason: entrypoint(s) failed: demo.Main: Metadata("bad marker"))']
        row = self.survey.summarise(self.report, log)[0]
        self.assertEqual(row["verdict"], "ran-partial")
        self.assertIn("demo.Main", row["reason"])
        self.assertEqual(
            self.survey.VERDICT.search("(partial) ran-partial").group(2),
            "ran-partial",
        )

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


class ContentItemsTest(unittest.TestCase):
    """The ids the survey gives for a content jar come from its baked package's declarations."""

    def setUp(self):
        self.survey = load_survey()

    def write_bake(self, game_dir, packages):
        mods = game_dir / "horizon-mods"
        mods.mkdir(parents=True)
        rows = []
        for jar, namespace, entries, declared in packages:
            path = mods / f"{namespace}.hmod"
            with zipfile.ZipFile(path, "w") as archive:
                for name, body in entries.items():
                    archive.writestr(name, body)
            rows.append({"jar": jar, "namespaces": [namespace], "error": None, "skipped": [],
                         "packages": [{"id": namespace, "namespace": namespace,
                                       "path": f"horizon-mods/{namespace}.hmod",
                                       "files": len(entries), "bytes": 1,
                                       "declarations": declared}]})
        report = game_dir / "config" / "horizon_java" / "bake-report.json"
        report.parent.mkdir(parents=True)
        report.write_text(json.dumps({"schema": 1, "jars": rows}))

    def test_items_and_block_items_are_given_and_itemless_blocks_are_not(self):
        with tempfile.TemporaryDirectory() as directory:
            game_dir = Path(directory)
            self.write_bake(game_dir, [
                ("ruby.jar", "ruby", {
                    "data/ruby/horizon/item/gem.json": '{"template":"minecraft:stick"}',
                    "data/ruby/horizon/block/gem_block.json":
                        '{"item":true,"template":"minecraft:stone"}',
                    "data/ruby/horizon/block/ghost.json":
                        '{"item":false,"template":"minecraft:stone"}',
                    "assets/ruby/lang/en_us.json": "{}",
                    "data/ruby/loot_table/blocks/gem_block.json": "{}",
                }, {"blocks": 2, "items": 1}),
                ("assets_only.jar", "pretty", {"assets/pretty/lang/en_us.json": "{}"},
                 {"blocks": 0, "items": 0}),
            ])
            found = self.survey.content_items(game_dir)
        self.assertEqual(found, {"ruby.jar": ["ruby:gem_block", "ruby:gem"]})

    def test_no_bake_report_means_no_content(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(self.survey.content_items(Path(directory)), {})


class FixtureManifestTest(unittest.TestCase):
    def test_absolute_jar_paths_and_the_batch_break_block(self):
        survey = load_survey()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            jar = root / "fixtures" / "m2c_fixture.jar"
            jar.parent.mkdir()
            jar.write_bytes(b"jar")
            path = root / "batches.json"
            path.write_text(json.dumps([
                {"batch": "fx", "break_block": "minecraft:stone",
                 "jars": [{"file": str(jar)}]},
                {"batch": "plain", "jars": [{"file": "a.jar"}]},
            ]))
            batches = survey.manifest(path)
            # An absolute path is used as given, not searched for under the corpora.
            self.assertEqual(survey.find_jar(root / "corpus", batches[0][1][0][1]), jar)
            with self.assertRaises(SystemExit):
                survey.find_jar(root / "corpus", str(root / "missing.jar"))
        self.assertEqual(survey.BREAK_BLOCKS["fx"], "minecraft:stone")
        self.assertEqual(survey.BREAK_BLOCKS["plain"], "minecraft:dirt")

    def test_break_waits_until_the_client_sees_the_placed_block(self):
        # `horizon-ctl block` replies as the game does: air until the server's update arrives.
        survey = load_survey()
        replies = iter([
            {"block": "minecraft:air", "loaded": True, "state": "minecraft:air"},
            None,  # a reply that did not parse
            {"block": "minecraft:stone", "loaded": True, "state": "minecraft:stone"},
        ])
        slept = []
        reads = survey.wait_for_block(lambda: next(replies), "minecraft:stone",
                                      sleep=slept.append)
        self.assertEqual(reads, 3)
        self.assertEqual(len(slept), 2)
        never = survey.wait_for_block(lambda: {"block": "minecraft:air"}, "minecraft:stone",
                                      tries=4, sleep=lambda _: None)
        self.assertIsNone(never)

    def test_a_dependency_shares_its_dependents_batch(self):
        # Since cross-jar linking a dependency outside the batch refuses the dependent, so the
        # sample keeps each declared dependency it carries beside the jar that needs it.
        survey = load_survey()
        batch_of = {name: batch for batch, jars in survey.SAMPLE for _, name, _ in jars}
        for dependent, dependency in [
            ("9f12a0dffd-stackrefill-26.3.0-5.1.jar", "bdc7223ca2-collective-26.3.0-8.67.jar"),
            ("fancymenu_neoforge_3.9.14_MC_26.3.jar", "konkrete_neoforge_1.11.1_MC_26.3.jar"),
            ("832882-8896904.jar", "326652-8890593.jar"),
        ]:
            self.assertEqual(batch_of[dependent], batch_of[dependency], dependent)


if __name__ == "__main__":
    unittest.main()
