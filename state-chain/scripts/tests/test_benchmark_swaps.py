import contextlib
import csv
import hashlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "benchmark_swaps", Path(__file__).resolve().parents[1] / "benchmark-swaps.py")
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class SummaryTests(unittest.TestCase):
    def setUp(self):
        self.components = {"n": 12, "c": 0, "a": 0, "v": 10}
        sample = {"components": list(self.components.items()), "extrinsic_time": 2_000_000,
                  "storage_root_time": 500_000, "reads": 3, "repeat_reads": 4,
                  "writes": 2, "repeat_writes": 5, "proof_size": 2048}
        self.batches = [{"benchmark": "swap_single_leg", "time_results": [sample, {
            **sample, "extrinsic_time": 4_000_000}], "db_results": [sample]}]

    def test_units_and_reference_cost(self):
        result = benchmark.summarize(self.batches, self.components, 8, 50)
        self.assertEqual(result["median_ms"], 3)
        self.assertEqual(result["max_ms"], 4)
        self.assertEqual(result["max_storage_root_ms"], 0.5)
        # Only distinct DB operations are charged; root time remains separate.
        self.assertAlmostEqual(result["reference_ms"], 4.124)
        self.assertEqual(result["proof_bytes"], 2048)

    def test_rejects_wrong_configuration(self):
        with self.assertRaises(ValueError):
            benchmark.summarize(self.batches, {**self.components, "c": 1}, 8, 50)

    def test_rejects_missing_measurements(self):
        for key in ["time_results", "db_results"]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                benchmark.summarize([{**self.batches[0], key: []}], self.components, 8, 50)

    def test_rejects_wrong_benchmark(self):
        with self.assertRaises(ValueError):
            benchmark.summarize([{**self.batches[0], "benchmark": "set_limit_order"}], self.components, 8, 50)

    def test_runner_pins_components_and_stops_at_budget(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "measurements"
            argv = ["benchmark-swaps.py", "--binary", __file__, "--output-dir", str(output),
                    "--binary-commit", "binary-head",
                    "--orders", "12", "100", "1000", "--pairs", "btc_sell",
                    "--configurations", "partial_many_lps", "--budget-ms", "15"]

            def run(command, **kwargs):
                low = next(arg.removeprefix("--low=") for arg in command if arg.startswith("--low="))
                self.assertIn(f"--high={low}", command)
                self.assertIn("--extra", command)
                self.assertNotIn("--no-verify", command)
                n, c, a, v = map(int, low.split(","))
                sample = {**self.batches[0]["time_results"][0],
                          "components": list(zip("ncav", [n, c, a, v])), "extrinsic_time": n * 100_000}
                raw = next(arg.removeprefix("--json-file=") for arg in command if arg.startswith("--json-file="))
                Path(raw).write_text(json.dumps([{"benchmark": "swap_single_leg",
                                                 "time_results": [sample], "db_results": [sample]}]))

            with patch("sys.argv", argv), patch.object(benchmark.subprocess, "run", side_effect=run) as runs, \
                    patch.object(benchmark.subprocess, "check_output", return_value="test-head"), \
                    contextlib.redirect_stdout(io.StringIO()):
                benchmark.main()
            self.assertEqual(runs.call_count, 2)
            with (output / "results.csv").open() as stream:
                rows = list(csv.DictReader(stream))
            self.assertEqual([row["n"] for row in rows], ["12", "100"])
            self.assertAlmostEqual(float(rows[0]["budget_cost_ms"]), 2.648)
            screen = json.loads((output / "budget-screen.json").read_text())[0]
            self.assertEqual(screen["largest_tested_n_below_budget"], 12)
            self.assertEqual(screen["first_tested_n_over_budget"], 100)
            manifest = json.loads((output / "manifest.json").read_text())
            self.assertEqual(manifest["status"], "complete")
            self.assertEqual(manifest["binary_commit"], "binary-head")
            self.assertEqual(manifest["git_head"], "test-head")
            self.assertEqual(manifest["binary_sha256"], hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
            report = (output / "summary.md").read_text()
            self.assertIn("Status: **complete**. Completed points: **2**.", report)
            self.assertIn("| btc_sell | partial_many_lps | 10 | 12 | 1.200 | 1.200 |", report)

    def test_runner_preserves_partial_results_on_failure(self):
        for error in [benchmark.subprocess.CalledProcessError(1, "node"),
                      benchmark.subprocess.TimeoutExpired("node", 600)]:
            with self.subTest(error=type(error).__name__), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "measurements"
                argv = ["benchmark-swaps.py", "--binary", __file__, "--output-dir", str(output),
                        "--orders", "12", "100", "--pairs", "btc_sell",
                        "--configurations", "partial_many_lps"]

                def run(command, **kwargs):
                    if "--low=100,0,0,10" in command:
                        raise error
                    raw = next(arg.removeprefix("--json-file=") for arg in command if arg.startswith("--json-file="))
                    Path(raw).write_text(json.dumps(self.batches))

                with patch("sys.argv", argv), patch.object(benchmark.subprocess, "run", side_effect=run), \
                        patch.object(benchmark.subprocess, "check_output", return_value="test-head"), \
                        contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                    benchmark.main()
                manifest = json.loads((output / "manifest.json").read_text())
                self.assertEqual(manifest["status"], "failed")
                self.assertEqual([run["status"] for run in manifest["runs"]], ["complete", "failed"])
                with (output / "results.csv").open() as stream:
                    self.assertEqual([row["n"] for row in csv.DictReader(stream)], ["12"])
                self.assertIn("Status: **failed**. Completed points: **1**.", (output / "summary.md").read_text())


if __name__ == "__main__":
    unittest.main()
