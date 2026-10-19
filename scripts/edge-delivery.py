"""Package the locally tested Edge development build; never publish or replace 0.6.12."""
from pathlib import Path
import base64, hashlib, json, lzma, shutil, struct, subprocess, zipfile

root=Path(__file__).resolve().parent.parent
out=root/'releases/v0.6.13'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
version='0.6.13'
proof=json.loads((root/'qa/v0613-edge/result.json').read_text(encoding='utf-8'))
exe=out/'Shiqian-v0.6.13-Windows-x64.exe'
assert proof['version']==version and proof['exeSha256']==sha(exe)==sha(root/'src-tauri/target/release/shiqian.exe')
assert not proof['errors'] and all(proof[k] for k in ['independentRegistrations','lastRegistrationRemoval','foreignProtected','legacyChrome','realEdgeNative','web','excerpt','image','manualTags','offlineRetry','idempotence','productionRegistryUntouched'])
assert '222 passed; 0 failed; 3 ignored' in (root/'docs/evidence/v0613-core.log').read_text(encoding='utf-8')
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert json.loads(subprocess.check_output(['git','show','HEAD:package.json'],cwd=root))['version']==version,'Commit the tested source before creating the source ZIP'
manifest=json.loads((root/'browser-extension/manifest.json').read_text(encoding='utf-8'));assert manifest['version']==version
digest=hashlib.sha256(base64.b64decode(manifest['key'])).digest()[:16];identifier=''.join(chr(97+(v>>4))+chr(97+(v&15)) for v in digest);assert identifier==proof['extensionId']=='emppinjophbaookhjdigajhimbafcplg'
for browser in ['edge','chrome']:
    source=out/'extensions'/browser
    actual=json.loads((source/'manifest.json').read_text(encoding='utf-8'))
    assert actual['version']==version and actual['key']==manifest['key'] and actual['permissions']==manifest['permissions']
    assert actual['background']==manifest['background'] and actual['action']==manifest['action']
    # The delivered worker uses the real host; the isolated QA host stays in qa/.
    assert (source/'background.js').read_bytes()==(root/'browser-extension/background.js').read_bytes()
    assert b'local.shiqian.capture.qa.' not in (source/'background.js').read_bytes()
    name='Edge' if browser=='edge' else 'Chrome'
    archive=out/f'Shiqian-v{version}-{name}-extension.zip'
    with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
        for p in sorted(source.rglob('*')):
            if p.is_file():z.write(p,f'Shiqian-{name}-extension/'+p.relative_to(source).as_posix())
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
        for p in source.rglob('*'):
            if p.is_file():assert z.read(f'Shiqian-{name}-extension/'+p.relative_to(source).as_posix())==p.read_bytes()
setup=root/'src-tauri/target/release/bundle/nsis/拾签_0.6.13_x64-setup.exe';data=setup.read_bytes();start=data.index(struct.pack('<I',0xDEADBEEF)+b'NullsoftInst')-4;offset=start+28;props=data[offset:offset+5];assert props[0]==0x5d
decoder=lzma.LZMADecompressor(format=lzma.FORMAT_RAW,filters=[{'id':lzma.FILTER_LZMA1,'dict_size':int.from_bytes(props[1:],'little'),'lc':3,'lp':0,'pb':2}]);payload=decoder.decompress(data[offset+5:],max_length=64*1024*1024);assert decoder.eof and payload.count(exe.read_bytes())==1
icon=(root/'src-tauri/icons/icon.ico').read_bytes();_,kind,count=struct.unpack_from('<HHH',icon);assert kind==1
for i in range(count):
    *_,length,pos=struct.unpack_from('<BBBBHHII',icon,6+i*16);frame=icon[pos:pos+length];assert frame in exe.read_bytes() and frame in data
shutil.copy2(setup,out/f'Shiqian-v{version}-Windows-x64-setup.exe')
shutil.copy2(root/'docs/release-v0.6.13.md',out/'Update-notes.zh-CN.md')
guide=(root/'browser-extension/README.md').read_text(encoding='utf-8').replace('(../README.md#chrome-插件--chrome-extension)','(https://github.com/SACO1F/Tagrove#readme)')
(out/'Edge-extension-guide.zh-CN-en.md').write_text(guide,encoding='utf-8')
subprocess.run(['git','archive','--format=zip','--prefix=Tagrove-v0.6.13/','--output='+str(out/'Shiqian-v0.6.13-source.zip'),head],cwd=root,check=True)
shutil.copy2(root/'THIRD_PARTY_NOTICES.md',out/'THIRD_PARTY_NOTICES.md')
(out/'BUILD-IDENTITY.md').write_text(f'''# Tagrove 0.6.13 Edge development delivery

- Source commit: {head}
- Desktop and extension version: {version}
- EXE SHA256: {sha(exe)}
- Installer SHA256: {sha(out/f'Shiqian-v{version}-Windows-x64-setup.exe')}
- Edge version tested: {proof['edgeVersion']}
- Fixed unpacked extension ID: {identifier}
- Core: 222 passed, 0 failed, 3 ignored
- Real Edge page/excerpt/public-image capture, tags, AI skip, offline retry and idempotence: passed
- Separate browser registrations, foreign ownership protection, legacy Chrome API: passed
- QA used a unique host key and isolated Edge profile; production registrations untouched; QA key removed
- Delivered extensions use the production host, with unchanged permissions
- Native EXE and installer icon frames: {count}/{count}; installer embeds exact tested EXE
- Frontend/localization and extension worker regressions: passed
- Local development delivery, not a new GitHub Release or Edge Add-ons store listing
- Installer installation/upgrade and managed-browser policy acceptance were not rerun
''',encoding='utf-8')
files=[p for p in sorted(out.rglob('*')) if p.is_file() and p.name!='SHA256SUMS.txt']
(out/'SHA256SUMS.txt').write_text('\n'.join(sha(p)+'  '+p.relative_to(out).as_posix() for p in files)+'\n',encoding='utf-8')
print(f'PASS Edge/Chrome ZIP content, source commit, extension identity/permissions, exact tested EXE payload, {count} icon frames and {len(files)} file hashes.')
