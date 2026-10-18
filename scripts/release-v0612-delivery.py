from pathlib import Path
import base64,hashlib,json,lzma,struct,subprocess,shutil,zipfile,re,tomllib
r=Path(__file__).resolve().parent.parent;version='0.6.12';out=r/'releases/v0.6.12';out.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
# Keep credentials and local/untracked libraries outside this archive: source is HEAD only.
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip()
committed=json.loads(subprocess.check_output(['git','show','HEAD:package.json'],cwd=r));assert committed['version']==version,'Commit the final release sources before packaging'
for rel in ['package.json','package-lock.json','src-tauri/tauri.conf.json','browser-extension/manifest.json']:
 data=json.loads((r/rel).read_text(encoding='utf-8'));assert data['version']==version,rel
 assert subprocess.check_output(['git','show','HEAD:'+rel],cwd=r).replace(b'\r\n',b'\n')==(r/rel).read_bytes().replace(b'\r\n',b'\n'),rel
lock=tomllib.loads((r/'src-tauri/Cargo.lock').read_text(encoding='utf-8'));assert next(p['version'] for p in lock['package'] if p['name']=='shiqian')==version
exe=out/'Shiqian-v0.6.12-Windows-x64.exe';assert sha(exe)==sha(r/'src-tauri/target/release/shiqian.exe')
identity=sha(exe)
brand=json.loads((r/'qa/v0612-brand/result.json').read_text(encoding='utf-8'));settings=json.loads((r/'qa/v0612-settings/result.json').read_text(encoding='utf-8'));floating=json.loads((r/'qa/v0612-floating/result.json').read_text(encoding='utf-8'))
assert brand['executableSha256']==settings['exeSha256']==floating['executableSha256']==identity
assert not brand['errors'] and not settings['errors'] and not floating['errors']
assert settings['gap']>=16 and all(settings[k] for k in ['animated','realFailedDocument','keyboard','interruptedToggle','reducedMotion','darkEnglish960'])
assert floating['collapseExpandCycles']==30 and brand['extension'] and brand['extensionQueue']
assert brand['brand']['corners']==brand['floating']['corners']==[0,0,0,0]
assert '216 passed; 0 failed; 3 ignored' in (r/'docs/evidence/v0612-core.log').read_text(encoding='utf-8')
icon=(r/'src-tauri/icons/icon.ico').read_bytes();_,kind,count=struct.unpack_from('<HHH',icon);assert kind==1;frames=[]
for i in range(count):
 *_,size,offset=struct.unpack_from('<BBBBHHII',icon,6+i*16);frames.append(icon[offset:offset+size])
assert all(x in exe.read_bytes() for x in frames)
setup=r/'src-tauri/target/release/bundle/nsis/拾签_0.6.12_x64-setup.exe';data=setup.read_bytes();start=data.index(struct.pack('<I',0xDEADBEEF)+b'NullsoftInst')-4;offset=start+28;props=data[offset:offset+5];assert props[0]==0x5d
decoder=lzma.LZMADecompressor(format=lzma.FORMAT_RAW,filters=[{'id':lzma.FILTER_LZMA1,'dict_size':int.from_bytes(props[1:],'little'),'lc':3,'lp':0,'pb':2}]);payload=decoder.decompress(data[offset+5:],max_length=64*1024*1024);assert decoder.eof and payload.count(exe.read_bytes())==1 and all(x in data for x in frames)
shutil.copy2(setup,out/'Shiqian-v0.6.12-Windows-x64-setup.exe')
manifest=json.loads((r/'browser-extension/manifest.json').read_text(encoding='utf-8'));digest=hashlib.sha256(base64.b64decode(manifest['key'])).digest()[:16];extension_id=''.join(chr(97+(v>>4))+chr(97+(v&15)) for v in digest);assert extension_id==brand['extensionId']=='emppinjophbaookhjdigajhimbafcplg'
for size,rel in manifest['icons'].items():assert struct.unpack('>II',(r/'browser-extension'/rel).read_bytes()[16:24])==(int(size),int(size))
with zipfile.ZipFile(out/'Shiqian-v0.6.12-Chrome-extension.zip','w',zipfile.ZIP_DEFLATED) as z:
 for p in sorted((r/'browser-extension').rglob('*')):
  if p.is_file():z.write(p,'Shiqian-Chrome-extension/'+p.relative_to(r/'browser-extension').as_posix())
subprocess.run(['git','archive','--format=zip','--prefix=Tagrove-v0.6.12/','--output='+str(out/'Shiqian-v0.6.12-source.zip'),head],cwd=r,check=True)
# Make local guide links work both in GitHub and next to the downloaded guides.
renames={'user-manual-v0.6.12.zh-CN.md':'Shiqian-v0.6.12-manual.zh-CN.md','user-manual-v0.6.12.en.md':'Shiqian-v0.6.12-manual.en.md','release-v0.6.12.md':'Shiqian-v0.6.12-release-notes.md','validation-v0.6.12.md':'Shiqian-v0.6.12-validation.md'}
for name,destination in renames.items():
 text=(r/'docs'/name).read_text(encoding='utf-8')
 for source,target in renames.items():text=text.replace('('+source+')','('+target+')')
 text=text.replace('(../README.md)','(https://github.com/SACO1F/Tagrove/tree/v0.6.12#readme)').replace('(../browser-extension/README.md)','(https://github.com/SACO1F/Tagrove/blob/v0.6.12/browser-extension/README.md)')
 (out/destination).write_text(text,encoding='utf-8')
shutil.copy2(r/'THIRD_PARTY_NOTICES.md',out/'THIRD_PARTY_NOTICES.md')
(out/'BUILD-IDENTITY.md').write_text(f'''# Tagrove v0.6.12 build identity / 构建身份

- Source commit / 源码提交: {head}
- Internal desktop/extension version / 两端版本: {version}
- EXE SHA256: {identity}
- Installer SHA256: {sha(out/'Shiqian-v0.6.12-Windows-x64-setup.exe')}
- Extension ID: {extension_id}
- Core: 216 passed, 0 failed, 3 ignored
- Frontend localization, CSP, TypeScript and production build: passed
- Desktop/Chrome branding, saved/default accents, index UI/keyboard/reduced motion, 30 palette collapse/expand cycles: passed
- Native EXE and installer icon resource frames: {count}/{count} match
- NSIS payload embeds the exact tested EXE: passed
- Source ZIP is the Git commit above, excluding unrelated local changes and user data
- Installer unsigned; payload checks do not represent fresh-install/upgrade acceptance
- Windows 11 x64 validation scope; Windows 10 acceptance incomplete

发布附件与 SHA256SUMS.txt 一起分发。Checksums verify integrity, not publisher identity.
''',encoding='utf-8')
for p in out.glob('*.zip'):
 with zipfile.ZipFile(p) as z:assert z.testzip() is None
files=[p for p in sorted(out.iterdir()) if p.is_file() and p.name!='SHA256SUMS.txt'];(out/'SHA256SUMS.txt').write_text('\n'.join(sha(p)+'  '+p.name for p in files)+'\n',encoding='utf-8')
print(f'PASS v{version}: committed sources, runtime versions, {identity[:12]}, 216 core tests, all desktop proof hashes, {count} native icon frames, exact NSIS EXE payload, extension identity and {len(files)} artifact hashes.')
