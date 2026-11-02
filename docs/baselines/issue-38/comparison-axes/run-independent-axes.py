from pathlib import Path
import types,unittest,hashlib
import argparse
parser=argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[4]
source=Path(__file__).with_name("independent-cases-unchanged.py").read_bytes()
module=types.ModuleType('independent_comparison_axes')
module.__file__=str(repo/'docs/baselines/issue-38-semantic-adapter/test_comparison_axes_7221910.py')
exec(compile(source,module.__file__,'exec'),module.__dict__)
module.OUT=args.output.resolve()
module.SEED=repo/'docs/baselines/issue-38/strict-validation/independent-latest-synthetic/test_control_complete_synthetic_cost_data_is_accepted/synthetic-unit-data'
print('unchanged_comparison_case_source_sha256',hashlib.sha256(source).hexdigest(),flush=True)
print('adapter_scope=portable retained valid seed path and output directory only; control and mutation bodies unchanged',flush=True)
result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(module.ComparisonAxes))
raise SystemExit(not result.wasSuccessful())
