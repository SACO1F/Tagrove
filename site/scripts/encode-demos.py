"""Encode actual compositor captures as 30 fps MP4 and 20 fps GIF demos.
Run after capture-english.cjs: python scripts/encode-demos.py
"""
from pathlib import Path
from PIL import Image
import json, subprocess, sys
import imageio_ffmpeg
site=Path(__file__).resolve().parents[1]
language='zh' if '--zh' in sys.argv else 'en'
run=Path((site/('scripts/latest-capture-zh.txt' if language=='zh' else 'scripts/latest-capture.txt')).read_text().strip())
out=site/'assets/ui'
ffmpeg=imageio_ffmpeg.get_ffmpeg_exe()
manifest=[]
def opaque(source):
    im=Image.open(source).convert('RGBA')
    return Image.alpha_composite(Image.new('RGBA',im.size,'#fffdf8'),im).convert('RGB')
def command(args):
    result=subprocess.run([ffmpeg,'-hide_banner','-loglevel','error','-y',*args],capture_output=True,text=True)
    if result.returncode: raise RuntimeError(result.stderr)
for source in sorted((run/'shots').glob('*.png')):
    opaque(source).save(out/(source.stem+'.webp'),'WEBP',quality=88,method=6)
for folder in sorted((run/'frames').iterdir()):
    timeline=json.loads((folder/'timeline.json').read_text())
    entries=[]
    for i,frame in enumerate(timeline):
        source=(folder/frame['file']).as_posix()
        entries.append("file '"+source+"'")
        delta=(timeline[i+1]['time']-frame['time']) if i+1<len(timeline) else .7
        entries.append('duration '+str(max(.016,delta)))
    entries.append("file '"+(folder/timeline[-1]['file']).as_posix()+"'")
    concat=folder/'frames.ffconcat';concat.write_text('\n'.join(entries)+'\n')
    target=out/(folder.name+'.mp4')
    command(['-f','concat','-safe','0','-i',str(concat),'-vf','fps=30,scale=trunc(iw/2)*2:trunc(ih/2)*2','-c:v','libx264','-preset','slow','-crf','20','-pix_fmt','yuv420p','-movflags','+faststart','-an',str(target)])
    gif=out/(folder.name+'.gif')
    command(['-i',str(target),'-filter_complex','fps=20,scale=min(1000\\,iw):-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle','-loop','0',str(gif)])
    check=Image.open(gif)
    assert check.n_frames>20,(folder.name,check.n_frames)
    metadata={'name':folder.name,'video':target.name,'videoBytes':target.stat().st_size,'videoFps':30,'gif':gif.name,'gifBytes':gif.stat().st_size,'gifFrames':check.n_frames,'gifFps':20,'captureFrames':len(timeline),'source':str(folder)}
    manifest.append(metadata)
    print(folder.name, len(timeline),'captured frames;',check.n_frames,'GIF frames;',round(target.stat().st_size/1024),'KB video,',round(gif.stat().st_size/1024),'KB GIF',flush=True)
(site/('assets/ui/chinese-demos.json' if language=='zh' else 'assets/ui/english-demos.json')).write_text(json.dumps({'run':str(run),'language':language,'source':'Actual app compositor capture with presentation pointer; synthetic library; no UI text replacements.','demos':manifest},indent=2)+'\n')
