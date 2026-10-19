"""Register a unique QA-only Edge host. Never touches the production host key."""
from pathlib import Path
import json,sys,winreg,os

root=Path(__file__).resolve().parent.parent
qa=root/'qa/v0613-edge'
manifest=qa/'edge-test-native-host.json'
host='local.shiqian.capture.qa.edge0613'
key=rf'Software\Microsoft\Edge\NativeMessagingHosts\{host}'
if sys.argv[1]=='register':
    data=json.loads(manifest.read_text(encoding='utf-8'))
    assert data['name']==host and data['type']=='stdio'
    executable=Path(os.environ.get('FLOATING_TEST_EXE',str(root/'releases/v0.6.13/Shiqian-v0.6.13-Windows-x64.exe'))).resolve()
    assert executable.is_relative_to((root/'releases').resolve())
    assert Path(data['path']).resolve()==executable
    development='chrome-extension://emppinjophbaookhjdigajhimbafcplg/'
    store='chrome-extension://oeeppgdhbgppkjgmjdaebffdhcfgeidi/'
    assert data['allowed_origins'] in [[development],[development,store]]
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,key) as handle:
            previous,kind=winreg.QueryValueEx(handle,'')
            assert previous==str(manifest) and kind==winreg.REG_SZ,'Unexpected QA key owner'
    except FileNotFoundError:
        pass
    with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER,key,0,winreg.KEY_SET_VALUE) as handle:
        winreg.SetValueEx(handle,'',0,winreg.REG_SZ,str(manifest))
    print('QA-only Edge host registered; production Chrome/Edge registrations untouched.')
elif sys.argv[1]=='unregister':
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,key) as handle:
            value,kind=winreg.QueryValueEx(handle,'')
            assert value==str(manifest) and kind==winreg.REG_SZ,'Refusing foreign key'
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER,key)
    except FileNotFoundError:
        pass
    print('QA-only Edge host removed.')
else:
    raise ValueError('Expected register or unregister')
