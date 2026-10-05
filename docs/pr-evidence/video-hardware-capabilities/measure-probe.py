import os,pathlib,subprocess,time,json,psutil,statistics
root=pathlib.Path('/workspace/scratch/serein-hw-capabilities')
env=dict(os.environ,LD_LIBRARY_PATH='/workspace/scratch/serein-openh264-isolation/native-runtime-packages/usr/lib/x86_64-linux-gnu:/workspace/scratch/serein-openh264-isolation/ffmpeg-prefix/lib')
samples=[]
for run in range(6):
 with (root/f'probe-measure-{run}.log').open('w') as log:
  start=time.monotonic();p=subprocess.Popen([str(root/'scanner')],env=env,stdout=log,stderr=log);parent=psutil.Process(p.pid)
  parent_peak=0;child_peak=0;max_children=0;seen=set()
  while p.poll() is None:
   try:
    parent_peak=max(parent_peak,parent.memory_info().rss)
    children=parent.children(recursive=True);max_children=max(max_children,len(children))
    for child in children:
     seen.add(child.pid)
     try:child_peak=max(child_peak,child.memory_info().rss)
     except psutil.Error:pass
   except psutil.Error:pass
   time.sleep(.001)
  elapsed=time.monotonic()-start;assert p.returncode==0
  samples.append({'run':run,'warmup':run==0,'elapsed_s':elapsed,'parent_peak_rss_bytes':parent_peak,'child_peak_rss_bytes':child_peak,'maximum_simultaneous_children':max_children,'children_observed':len(seen)})
result={'method':'One warmup and five fresh headless synthetic scans of actual Detector and linked FFmpeg. RSS polled every1ms; sampled peaks may miss short-lived allocations. No usable GPU devices.','runs':samples,'median_elapsed_s':statistics.median(s['elapsed_s'] for s in samples[1:]),'median_parent_peak_rss_bytes':statistics.median(s['parent_peak_rss_bytes'] for s in samples[1:]),'median_child_peak_rss_bytes':statistics.median(s['child_peak_rss_bytes'] for s in samples[1:])}
(root/'probe-measurements.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
