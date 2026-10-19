// Capture real UI only, using the isolated QA library and QA-only native host.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const crypto=require('node:crypto');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=path.resolve(__dirname,'..'),qa=path.join(root,'qa/v0613-edge');
const host='local.shiqian.capture.qa.edge0613';
const storeIdentity=process.env.SHIQIAN_QA_STORE_IDENTITY==='1';
const id=storeIdentity?'oeeppgdhbgppkjgmjdaebffdhcfgeidi':'emppinjophbaookhjdigajhimbafcplg';
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
  const extension=path.join(qa,storeIdentity?'edge-store-key-extension':'edge-extension');
  fs.cpSync(path.join(root,'releases/v0.6.13/extensions/edge'),extension,{recursive:true});
  if(storeIdentity){
    const key=crypto.createPublicKey(fs.readFileSync(path.join(root,'scripts/fixtures/edge-store-public-key.pem'))).export({type:'spki',format:'der'});
    const derived=[...crypto.createHash('sha256').update(key).digest().subarray(0,16)].map(b=>String.fromCharCode(97+(b>>4),97+(b&15))).join('');
    assert.equal(derived,id);
    const file=path.join(extension,'manifest.json'),value=JSON.parse(fs.readFileSync(file));value.key=key.toString('base64');fs.writeFileSync(file,JSON.stringify(value,null,2));
  }
  const worker=path.join(extension,'background.js');fs.writeFileSync(worker,fs.readFileSync(worker,'utf8').replace('const HOST = "local.shiqian.capture";',`const HOST = "${host}";`));
  edge=await chromium.launchPersistentContext(path.join(qa,storeIdentity?'edge-store-key-profile':'edge-profile'),{executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true,viewport:{width:1280,height:800},env:process.env,ignoreDefaultArgs:['--disable-extensions'],args:['--enable-unsafe-extension-debugging','--disable-background-networking',`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
  try{const loaded=await(await edge.browser().newBrowserCDPSession()).send('Extensions.loadUnpacked',{path:extension});assert.equal(loaded.id,id);}catch{}
  const popup=await edge.newPage();await popup.goto(`chrome-extension://${id}/popup.html`);
  assert.equal(await popup.evaluate(()=>chrome.runtime.id),id);
  const status=await popup.evaluate(()=>chrome.runtime.sendMessage({action:'status'}));assert(status.ok&&status.connected);
  await popup.locator('#refresh-tags').click();
  await popup.getByRole('checkbox',{name:'Edge 验收标签',exact:true}).check();
  await popup.locator('#title').fill('网页学习资料');await popup.locator('#url').fill('https://example.com/tagrove-store-demo');
  await popup.locator('#submit').click();await popup.locator('#result').filter({hasText:/已保存到拾签|资料已存在/}).waitFor();
  if(storeIdentity){
    const send=message=>popup.evaluate(message=>chrome.runtime.sendMessage(message),message);
    const catalog=await send({action:'tags'}),tag=catalog.tags.find(t=>t.name==='Edge 验收标签');assert(tag);
    const saved=await send({action:'submit',capture:{kind:'web_excerpt',title:'商店身份摘录验收',url:'https://example.com/tagrove-store-excerpt',excerpt:'公钥身份验收：学习资料与建筑外墙。',tagIds:[tag.id]}});
    assert.equal(saved.record.status,'done');assert.equal(saved.record.result.aiQueued,false);
    const file=await api('file',{id:saved.record.result.id});assert(file.tags.some(t=>t.id===tag.id));
    const session=path.join(qa,'library/bridge-session.json'),hold=path.join(qa,'library/bridge-session.store-key-hold');fs.renameSync(session,hold);
    let pending;try{pending=await send({action:'submit',capture:{kind:'web_page',title:'商店身份离线验收',url:'https://example.com/tagrove-store-offline',tagIds:[tag.id]}});assert.equal(pending.record.status,'failed');assert.equal(pending.record.errorCode,'BRIDGE_OFFLINE');}finally{fs.renameSync(hold,session);}
    await send({action:'retry',id:pending.record.id});const queue=await send({action:'list'});assert.equal(queue.queue.find(r=>r.id===pending.record.id).status,'done');
    const before=(await api('query',{limit:100})).total;await send({action:'retry',id:pending.record.id});assert.equal((await api('query',{limit:100})).total,before);
  }
  const prefix=storeIdentity?'store-key-':'edge-';
  await popup.evaluate(()=>scrollTo(0,0));await popup.screenshot({path:path.join(qa,prefix+'capture-tags.png'),animations:'disabled'});
  await popup.goto(`chrome-extension://${id}/queue.html`);await popup.locator('#queue').waitFor();await popup.evaluate(()=>scrollTo(0,0));await popup.screenshot({path:path.join(qa,prefix+'queue.png'),animations:'disabled'});
  fs.writeFileSync(path.join(qa,storeIdentity?'store-public-key-result.json':'store-capture-result.json'),JSON.stringify({version:status.version,exeSha256:crypto.createHash('sha256').update(fs.readFileSync(process.env.FLOATING_TEST_EXE)).digest('hex'),edgeVersion:await edge.browser().version(),extensionId:id,publicKeyIdentity:storeIdentity,realNativeConnection:true,pageCapture:true,manualTag:true,excerpt:storeIdentity,offlineRetry:storeIdentity,idempotence:storeIdentity,screenshotSize:[1280,800],productionRegistryUntouched:true,storeSignedInstallation:false},null,2));
  console.log('PASS real connected capture and queue screenshots, 1280x800, synthetic QA library.');
}finally{
  if(edge)await edge.close();if(registered)run('python','scripts/edge-native-test-host.py','unregister');if(desktop)await desktop.close();run(process.execPath,'scripts/close-qa.cjs');
}})().catch(error=>{console.error(error);process.exitCode=1;});
