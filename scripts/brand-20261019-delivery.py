from pathlib import Path
import base64,hashlib,json,lzma,struct,shutil,zipfile
r=Path(__file__).resolve().parent.parent
out=r/'releases/v0.6.11-brand.1'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
source=r/'public/icon.png'
assert sha(source)=='b0395b75f081685aa302b97e5c48a9295f1d13ff095a9f1d80325d97b66b0c5b'
from PIL import Image
image=Image.open(source);assert image.mode=='RGBA' and image.getchannel('A').histogram()[0] > image.width*image.height*.3
exe=out/'Shiqian-v0.6.11-brand.1-Windows-x64.exe'
result=json.loads((r/'qa/brand-20261019/result.json').read_text(encoding='utf-8'))
assert result['executableSha256']==sha(exe)==sha(r/'src-tauri/target/release/shiqian.exe')
assert result['brand']['corners']==[0,0,0,0] and result['floating']['corners']==[0,0,0,0]
assert result['sourceSha256']==sha(source) and all(result[k] for k in ['sidebarCollapsed','dark','english','extension','extensionQueue']) and not result['errors']
icon=(r/'src-tauri/icons/icon.ico').read_bytes();reserved,kind,count=struct.unpack_from('<HHH',icon);assert reserved==0 and kind==1 and count>=5
frames=[]
for i in range(count):
 w,h,_,_,_,_,size,offset=struct.unpack_from('<BBBBHHII',icon,6+i*16);frame=icon[offset:offset+size];assert len(frame)==size;frames.append((w or 256,h or 256,frame))
assert any(w==256 for w,h,b in frames)
exe_frames=sum(b in exe.read_bytes() for w,h,b in frames);assert exe_frames>=1
manifest=json.loads((r/'browser-extension/manifest.json').read_text(encoding='utf-8'))
for size,rel in manifest['icons'].items():assert struct.unpack('>II',(r/'browser-extension'/rel).read_bytes()[16:24])==(int(size),int(size))
digest=hashlib.sha256(base64.b64decode(manifest['key'])).digest()[:16]
ext_id=''.join(chr(97+(v>>4))+chr(97+(v&15)) for v in digest)
assert ext_id==result['extensionId']==(r/'browser-extension/extension-id.txt').read_text().strip()
setup=r/'src-tauri/target/release/bundle/nsis/拾签_0.6.11_x64-setup.exe';data=setup.read_bytes();start=data.index(struct.pack('<I',0xDEADBEEF)+b'NullsoftInst')-4;offset=start+28
props=data[offset:offset+5];assert props[0]==0x5d
decoder=lzma.LZMADecompressor(format=lzma.FORMAT_RAW,filters=[{'id':lzma.FILTER_LZMA1,'dict_size':int.from_bytes(props[1:],'little'),'lc':3,'lp':0,'pb':2}]);payload=decoder.decompress(data[offset+5:],max_length=64*1024*1024)
assert decoder.eof and payload.count(exe.read_bytes())==1
installer_frames=sum(b in data for w,h,b in frames);assert installer_frames>=1
shutil.copy2(setup,out/'Shiqian-v0.6.11-brand.1-Windows-x64-setup.exe')
shutil.copy2(r/'docs/brand-update-2026-10-19.zh-CN.md',out/'Update-notes.zh-CN.md')
shutil.copy2(r/'THIRD_PARTY_NOTICES.md',out/'THIRD_PARTY_NOTICES.md')
shutil.copy2(source,out/'Tagrove-logo-transparent.png')
shutil.copy2(r/'src-tauri/icons/icon.ico',out/'Tagrove.ico')
zip_path=out/'Shiqian-v0.6.11-brand.1-Chrome-extension.zip'
with zipfile.ZipFile(zip_path,'w',zipfile.ZIP_DEFLATED) as z:
 for p in sorted((r/'browser-extension').rglob('*')):
  if p.is_file():z.write(p,'Shiqian-Chrome-extension/'+p.relative_to(r/'browser-extension').as_posix())
with zipfile.ZipFile(zip_path) as z:
 assert z.testzip() is None
 for p in (r/'browser-extension').rglob('*'):
  if p.is_file():assert z.read('Shiqian-Chrome-extension/'+p.relative_to(r/'browser-extension').as_posix())==p.read_bytes()
identity=f'''# v0.6.11-brand.1 / 2026-10-19

默认主题调整为群青，保留已保存的个人主题颜色。桌面与扩展采用透明 Tagrove Logo。应用内部版本仍为 0.6.11，应用身份及 Chrome 扩展 ID 保持兼容。

- Transparent PNG: 1254 × 1254; SHA256 {sha(source)}
- EXE SHA256: {sha(exe)}
- Installer SHA256: {sha(out/'Shiqian-v0.6.11-brand.1-Windows-x64-setup.exe')}
- Desktop asset bytes match original; sidebar expanded/collapsed, dark/English, floating 28px: passed
- ICO frames: {count}; matching native EXE resource frames: {exe_frames}; matching installer resource frames: {installer_frames}
- Real Chrome extension load, icons, popup and queue headers, original extension ID: passed
- Built-in image_gen background extraction; RGBA alpha preserved; previous opaque source remains in the preceding release; no new runtime dependencies
- Frontend production build and localization: passed
- Installer payload contains the exact tested EXE; install/upgrade not rerun this round
- This round changes logo assets and presentation; prior core and floating width fixes are retained
'''
(out/'BUILD-IDENTITY.zh-CN.md').write_text(identity,encoding='utf-8')
shutil.copy2(r/'docs/logo-transparent-prompt-2026-10-18.txt',out/'Image-edit-prompt.txt')
files=[p for p in sorted(out.rglob('*')) if p.is_file() and p.name!='SHA256SUMS.txt']
(out/'SHA256SUMS.txt').write_text('\n'.join(sha(p)+'  '+p.relative_to(out).as_posix() for p in files)+'\n',encoding='utf-8')
for line in (out/'SHA256SUMS.txt').read_text(encoding='utf-8').splitlines():
 digest,name=line.split('  ',1);assert sha(out/name)==digest
print(f'PASS transparent PNG identity and alpha; {exe_frames}/{count} icon frames embedded in EXE and {installer_frames}/{count} in installer; tested EXE payload; extension identity/ZIP; {len(files)} artifact hashes.')
