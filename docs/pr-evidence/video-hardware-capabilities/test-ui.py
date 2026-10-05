import importlib.util,json,os,subprocess
from pathlib import Path
spec=importlib.util.spec_from_file_location('native', '/workspace/scratch/serein-openh264-isolation/build-voice-tests.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
m.CACHE=Path('/workspace/scratch/serein-ui-preview/target/debug')
fp=max(m.CACHE.glob('.fingerprint/ui-*/lib-ui.json'), key=lambda p:p.stat().st_mtime)
deps=m.dependencies(fp)
helper=max(m.CACHE.glob('.fingerprint/serein-ui-task-preview-*/bin-serein-ui-task-preview.json'),key=lambda p:p.stat().st_mtime)
deps['test_support']=m.dependencies(helper)['test_support']
base=['/home/agent/.cargo/bin/rustc','--crate-name','ui','--edition=2024','--test','/workspace/Serein/crates/ui/src/lib.rs','--cfg','feature="demo"','--cfg','feature="default"','-C','opt-level=0','-C','debuginfo=0','-L','dependency='+str(m.CACHE/'deps')]
for name,data in deps.items(): base+=['--extern',name+'='+data['artifact']]
out=Path('/workspace/scratch/serein-hw-capabilities')
env=dict(os.environ,CARGO_MANIFEST_DIR='/workspace/Serein/crates/ui',CARGO_PKG_VERSION='0.1.0',CARGO_PKG_NAME='ui')
cmd=base+['-o',str(out/'ui-tests')]
(out/'ui-test-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
subprocess.run(cmd,cwd='/workspace/Serein',env=env,check=True)
subprocess.run([str(out/'ui-tests'),'--test-threads=2'],cwd='/workspace/Serein',env=env,check=True)
base[0]='/home/agent/.cargo/bin/clippy-driver'
subprocess.run(base+['--emit=metadata','-o',str(out/'ui-test-clippy.rmeta'),'-D','warnings'],cwd='/workspace/Serein',env=env,check=True)
