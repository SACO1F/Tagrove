const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {execFileSync}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=path.resolve(__dirname,'..'),qa=path.join(root,'qa/v0613-browser-guide');
const store='oeeppgdhbgppkjgmjdaebffdhcfgeidi',url=`https://microsoftedge.microsoft.com/addons/detail/${store}`;
const run=(file,...args)=>execFileSync(file,args,{cwd:root,env:process.env,windowsHide:true,encoding:'utf8'});
function ping(){const msg=Buffer.from(JSON.stringify({protocol:1,action:'ping'})),len=Buffer.alloc(4);len.writeUInt32LE(msg.length);const b=execFileSync(process.env.FLOATING_TEST_EXE,[`chrome-extension://${store}/`],{cwd:root,env:process.env,windowsHide:true,input:Buffer.concat([len,msg]),timeout:15000});return JSON.parse(b.subarray(4,4+b.readUInt32LE()));}
let desktop,main;const errors=[];
(async()=>{try{
  assert.equal(path.resolve(process.env.SHIQIAN_QA_DATA_DIR),path.join(qa,'library'));assert.equal(process.env.SHIQIAN_QA_BRIDGE_NO_REGISTRY,'1');
  fs.mkdirSync(qa,{recursive:true});run(process.execPath,'scripts/launch-qa.cjs',process.env.FLOATING_TEST_EXE);
  desktop=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.SHIQIAN_CDP_PORT);main=desktop.contexts()[0].pages().find(p=>!p.url().includes('floating'));
  main.on('pageerror',e=>errors.push(String(e)));await main.emulateMedia({reducedMotion:'reduce'});await main.locator('#workspace-sidebar').waitFor();
  const api=(action,payload={})=>main.evaluate(({action,payload})=>window.__TAURI_INTERNALS__.invoke('api',{action,payload}),{action,payload});
  const boot=await api('bootstrap');for(const browser of ['edge','chrome']){if((await api('capture.bridge.status',{browser})).removable)await api('capture.bridge.unregister',{browser});}
  await api('settings.save',{key:'language',value:'zh-CN'});await api('settings.save',{key:'theme',value:'light'});
  await main.evaluate(library=>{localStorage.removeItem('shiqian-onboarding:v1:'+library);localStorage.removeItem('tagrove-extension-guide:v1:'+library+':edge');},boot.dataPath);
  await main.reload();await main.locator('#workspace-sidebar').waitFor();
  // In explicit QA mode only, the native OS opener records its exact URL instead of opening the user's browser.
  const openRecord=path.join(qa,'library/qa-store-open.json');
  if(fs.existsSync(openRecord))fs.unlinkSync(openRecord);
  await main.getByRole('button',{name:'暂不设置',exact:true}).click();await main.getByRole('button',{name:'安装 Edge 扩展',exact:true}).waitFor();
  assert.equal(fs.existsSync(openRecord),false);
  await main.screenshot({path:path.join(qa,'first-run-light.png'),animations:'disabled'});
  await main.getByRole('button',{name:'安装 Edge 扩展',exact:true}).click();
  for(let i=0;i<50&&!fs.existsSync(openRecord);i++)await main.waitForTimeout(100);
  assert.equal(JSON.parse(fs.readFileSync(openRecord,'utf8')).url,url);
  await main.getByRole('button',{name:'稍后',exact:true}).click();assert.equal(await main.getByRole('button',{name:'安装 Edge 扩展',exact:true}).count(),0);
  await main.getByRole('button',{name:'开始使用',exact:true}).click();
  async function preferences(language='zh-CN'){
    await main.getByRole('button',{name:language==='en'?'Preferences':'偏好设置',exact:true}).click();
    await main.getByRole('button',{name:language==='en'?'Browser capture':'浏览器采集',exact:true}).click();
  }
  await preferences();await main.getByRole('button',{name:'检查连接',exact:true}).click();await main.getByText('请先在下方连接或修复 Edge，再打开扩展检查连接。',{exact:true}).waitFor();
  await main.getByRole('button',{name:'连接 Edge 扩展',exact:true}).click();
  const ready=await api('capture.bridge.status',{browser:'edge'});assert(ready.registered);assert.equal(ready.storeLastSeen,0);
  await main.getByRole('button',{name:'检查连接',exact:true}).click();await main.getByText('本机连接已就绪。请在 Edge 扩展中点击“检查连接”，完成通信验证。',{exact:true}).waitFor();
  await main.locator('.browser-install-guide').scrollIntoViewIfNeeded();await main.screenshot({path:path.join(qa,'settings-light.png'),animations:'disabled'});
  await main.getByRole('button',{name:'不再提醒',exact:true}).click();
  const prefKey='tagrove-extension-guide:v1:'+boot.dataPath+':edge';assert.equal(await main.evaluate(key=>localStorage.getItem(key),prefKey),'never');
  await main.getByRole('button',{name:'关闭对话框',exact:true}).click();await preferences();assert.equal(await main.getByRole('button',{name:'安装与连接指南',exact:true}).getAttribute('aria-expanded'),'false');
  await main.getByRole('button',{name:'安装与连接指南',exact:true}).click();assert(ping().ok);
  await main.getByRole('button',{name:'已验证扩展通信',exact:true}).waitFor();assert.equal(await main.getByRole('button',{name:'已验证扩展通信',exact:true}).getAttribute('aria-expanded'),'false');
  const seen=await api('capture.bridge.status',{browser:'edge'});assert(seen.storeLastSeen>0);assert.equal((await api('capture.bridge.status',{browser:'chrome'})).storeLastSeen,0);
  await api('capture.bridge.register',{browser:'edge'});assert.equal((await api('capture.bridge.status',{browser:'edge'})).storeLastSeen,seen.storeLastSeen);
  await main.getByRole('button',{name:'关闭对话框',exact:true}).click();
  await api('settings.save',{key:'language',value:'en'});await api('settings.save',{key:'theme',value:'dark'});await api('settings.save',{key:'uiFontSize',value:16});await main.reload();await main.locator('#workspace-sidebar').waitFor();
  await preferences('en');await main.getByRole('button',{name:'Extension communication verified',exact:true}).click();
  await main.setViewportSize({width:1000,height:780});await main.locator('.browser-install-guide').scrollIntoViewIfNeeded();
  await main.getByRole('button',{name:'Install Edge extension',exact:true}).waitFor();await main.screenshot({path:path.join(qa,'settings-english-dark.png'),animations:'disabled'});
  const overflow=await main.locator('.browser-guide-content').evaluate(el=>el.scrollWidth>el.clientWidth+1);assert.equal(overflow,false);
  assert.equal(await main.getByRole('dialog').count(),1);
  await main.getByRole('button',{name:'Close dialog',exact:true}).click();await main.reload();await main.locator('#workspace-sidebar').waitFor();assert.equal(await main.getByRole('dialog').count(),0);
  assert.deepEqual(errors,[]);
  fs.writeFileSync(path.join(qa,'result.json'),JSON.stringify({exeSha256:crypto.createHash('sha256').update(fs.readFileSync(process.env.FLOATING_TEST_EXE)).digest('hex'),firstRunOptional:true,noAutomaticBrowserOpen:true,osUrlOpenerSuppressedInQa:true,later:true,neverPersists:true,registrationDoesNotImplyInstallation:true,realNativeReceipt:true,repairPreservesReceipt:true,edgeReceiptDoesNotVerifyChrome:true,verifiedStopsReminder:true,englishDarkLargeFont:true,narrowNoOverflow:true,noStartupModalAfterCompletion:true,errors},null,2));
  console.log('PASS optional first-run guide, explicit install action, Later/Never, real connection receipt, repair persistence, English/dark/narrow layout and no repeat startup modal.');
}finally{if(desktop)await desktop.close();run(process.execPath,'scripts/close-qa.cjs');}})().catch(e=>{console.error(e);process.exitCode=1;});
