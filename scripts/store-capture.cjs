// Capture real UI only, using the isolated QA library and QA-only native host.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=path.resolve(__dirname,'..'),qa=path.join(root,'qa/v0613-edge');
const host='local.shiqian.capture.qa.edge0613',id='emppinjophbaookhjdigajhimbafcplg';
const run=(file,...args)=>execFileSync(file,args,{cwd:root,env:process.env,windowsHide:true,encoding:'utf8'});
let desktop,edge,registered=false;
(async()=>{try{
  assert.equal(path.resolve(process.env.SHIQIAN_QA_DATA_DIR),path.join(qa,'library'));
  assert.equal(process.env.SHIQIAN_QA_BRIDGE_NO_REGISTRY,'1');
  run(process.execPath,'scripts/launch-qa.cjs',process.env.FLOATING_TEST_EXE);
  desktop=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.SHIQIAN_CDP_PORT);
  const main=desktop.contexts()[0].pages().find(p=>!p.url().includes('floating'));
  await main.locator('#workspace-sidebar').waitFor();
  const api=(action,payload={})=>main.evaluate(({action,payload})=>window.__TAURI_INTERNALS__.invoke('api',{action,payload}),{action,payload});
  await api('capture.bridge.register',{browser:'edge'});
  const manifest=JSON.parse(fs.readFileSync(path.join(qa,'bridge/native-host.json'),'utf8'));
  manifest.name=host;fs.writeFileSync(path.join(qa,'edge-test-native-host.json'),JSON.stringify(manifest));
  run('python','scripts/edge-native-test-host.py','register');registered=true;
  const extension=path.join(qa,'edge-extension');
  fs.cpSync(path.join(root,'releases/v0.6.13/extensions/edge'),extension,{recursive:true});
  const worker=path.join(extension,'background.js');fs.writeFileSync(worker,fs.readFileSync(worker,'utf8').replace('const HOST = "local.shiqian.capture";',`const HOST = "${host}";`));
  edge=await chromium.launchPersistentContext(path.join(qa,'edge-profile'),{executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true,viewport:{width:1280,height:800},env:process.env,ignoreDefaultArgs:['--disable-extensions'],args:['--enable-unsafe-extension-debugging','--disable-background-networking',`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
  try{await(await edge.browser().newBrowserCDPSession()).send('Extensions.loadUnpacked',{path:extension});}catch{}
  const popup=await edge.newPage();await popup.goto(`chrome-extension://${id}/popup.html`);
  const status=await popup.evaluate(()=>chrome.runtime.sendMessage({action:'status'}));assert(status.ok&&status.connected);
  await popup.locator('#refresh-tags').click();
  await popup.getByRole('checkbox',{name:'Edge 验收标签',exact:true}).check();
  await popup.locator('#title').fill('网页学习资料');await popup.locator('#url').fill('https://example.com/tagrove-store-demo');
  await popup.locator('#submit').click();await popup.locator('#result').filter({hasText:/已保存到拾签|资料已存在/}).waitFor();
  await popup.evaluate(()=>scrollTo(0,0));await popup.screenshot({path:path.join(qa,'edge-capture-tags.png'),animations:'disabled'});
  await popup.goto(`chrome-extension://${id}/queue.html`);await popup.locator('#queue').waitFor();await popup.evaluate(()=>scrollTo(0,0));await popup.screenshot({path:path.join(qa,'edge-queue.png'),animations:'disabled'});
  fs.writeFileSync(path.join(qa,'store-capture-result.json'),JSON.stringify({version:status.version,realNativeConnection:true,pageCapture:true,manualTag:true,screenshotSize:[1280,800],productionRegistryUntouched:true}));
  console.log('PASS real connected capture and queue screenshots, 1280x800, synthetic QA library.');
}finally{
  if(edge)await edge.close();if(registered)run('python','scripts/edge-native-test-host.py','unregister');if(desktop)await desktop.close();run(process.execPath,'scripts/close-qa.cjs');
}})().catch(error=>{console.error(error);process.exitCode=1;});
