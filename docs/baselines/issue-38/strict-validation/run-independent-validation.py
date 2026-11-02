from pathlib import Path
import types,hashlib,json,unittest,shutil,sys
import argparse
parser=argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[4]
source=Path(__file__).with_name("independent-cases-unchanged.py").read_bytes()
module=types.ModuleType('retained_independent_aggregation')
module.__file__=str(repo/'docs/baselines/issue-38-semantic-adapter/test_aggregation_independent.py')
exec(compile(source,module.__file__,'exec'),module.__dict__)
cls=module.IndependentRejectionTests
original=cls.setUp
archive=args.output.resolve()
archive.mkdir(parents=True)
def adapted_set_up(self):
 original(self)
 # Only add required collector schema/verified ownership fields. The seven
 # independent positive/rejection case bodies and their mutations are unchanged.
 data=json.loads((self.run/'collector.json').read_text())
 data.update(StderrReaderFinished=True,AppCreatedTicks=1000,ExternalBeginObservationMs=0,ExternalDurationMs=7000,ExternalSceneSnapshotReadyMs=0)
 self.write_json(self.run/'collector.json',data)
 (self.run/'owned-tree.csv').write_text('ProcessId,ParentProcessId,Name,CreatedTicks,Role\n99,0,synthetic,1000,app\n100,99,synthetic,1001,fixture-child\n101,99,synthetic,1002,fixture-child\n')
def retain(self):
 target=archive/self._testMethodName
 target.mkdir()
 shutil.copytree(self.root,target/'synthetic-unit-data')
cls.setUp=adapted_set_up
cls.tearDown=retain
print('unchanged_independent_test_source_sha256',hashlib.sha256(source).hexdigest(),flush=True)
print('adapter_scope=required collector fields and verified ownership snapshot; test bodies unchanged; all data synthetic',flush=True)
result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(cls))
raise SystemExit(not result.wasSuccessful())
