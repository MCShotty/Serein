import importlib.util, json, os, subprocess
from pathlib import Path
out=Path('/workspace/scratch/serein-hw-capabilities')
spec=importlib.util.spec_from_file_location('native',str(out/'native/build-voice-tests.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
link=json.loads((out/'native/voice-current-source-build/link-context.json').read_text())
deps=m.dependencies(Path(link['ui_fingerprint']),m.UI_CACHE,{'ui','model'})
# Type-only bridge: exact real egui Context, without eframe's unrelated native window
# dependencies colliding with the independently cached voice/platform proc macros.
ui_fp=max(m.UI_CACHE.glob('.fingerprint/ui-*/lib-ui.json'),key=lambda p:p.stat().st_mtime)
egui=m.dependencies(ui_fp,m.UI_CACHE,{'egui'})['egui']['artifact']
(out/'eframe-probe-types.rs').write_text('pub use egui;\n')
subprocess.run(['/home/agent/.cargo/bin/rustc','--crate-name','eframe_probe_types','--edition=2024','--crate-type=rlib',str(out/'eframe-probe-types.rs'),'--extern','egui='+egui,'-L','dependency='+str(m.UI_CACHE/'deps'),'-o',str(out/'libeframe_probe_types.rlib')],check=True)
deps['eframe']={'artifact':str(out/'libeframe_probe_types.rlib')}
cmd=['/home/agent/.cargo/bin/rustc','--crate-name','scanner','--edition=2024',str(out/'scanner.rs'),'-C','debuginfo=0','-C','opt-level=1']
for name,data in deps.items():cmd+=['--extern',name+'='+data['artifact']]
cmd+=['--extern','discord_voice='+link['voice_library']]
for directory in reversed(link['dependency_search']):cmd+=['-L','dependency='+directory]
for directory in link['native_search']:cmd+=['-L','native='+directory]
for archive in (m.UI_CACHE/'build').glob('*/out/**/*.a'):cmd+=['-L','native='+str(archive.parent)]
cmd+=link['native_flags']
(out/'scanner-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
subprocess.run(cmd+['--test','-o',str(out/'scanner-tests')],check=True,cwd=m.REPO)
subprocess.run(cmd+['-o',str(out/'scanner')],check=True,cwd=m.REPO)
cmd[0]='/home/agent/.cargo/bin/clippy-driver'
subprocess.run(cmd+['--test','--emit=metadata','-o',str(out/'scanner-clippy.rmeta'),'-D','warnings'],check=True,cwd=m.REPO)
