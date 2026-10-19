const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {execFileSync}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=path.resolve(__dirname,'..'),qa=path.join(root,'qa/v0613-edge'),host='local.shiqian.capture.qa.edge0613';
let desktop,main,edge,popup,hostRegistered=false;const errors=[];
const api=(action,payload={})=>main.evaluate(({action,payload})=>window.__TAURI_INTERNALS__.invoke('api',{action,payload}),{action,payload});
const call=(action,extra={})=>popup.evaluate(message=>chrome.runtime.sendMessage(message),{action,...extra});
async function until(fn,description){for(let i=0;i<120;i++){if(await fn())return;await main.waitForTimeout(100);}throw Error(description);}
const run=(file,...args)=>execFileSync(file,args,{cwd:root,env:process.env,windowsHide:true,encoding:'utf8'});
function native(action,capture){const message=Buffer.from(JSON.stringify({protocol:1,action,capture}));const length=Buffer.alloc(4);length.writeUInt32LE(message.length);const result=execFileSync(process.env.FLOATING_TEST_EXE,['chrome-extension://emppinjophbaookhjdigajhimbafcplg/'],{cwd:root,env:process.env,windowsHide:true,input:Buffer.concat([length,message]),timeout:45000});return JSON.parse(result.subarray(4,4+result.readUInt32LE()));}
(async()=>{try{
  assert.equal(path.resolve(process.env.SHIQIAN_QA_DATA_DIR),path.join(qa,'library'));
  fs.mkdirSync(qa,{recursive:true});
  run(process.execPath,'scripts/launch-qa.cjs',process.env.FLOATING_TEST_EXE);
  desktop=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.SHIQIAN_CDP_PORT);
  main=desktop.contexts()[0].pages().find(p=>!p.url().includes('floating'));main.on('pageerror',e=>errors.push(String(e)));
  await main.locator('#workspace-sidebar').waitFor();const boot=await api('bootstrap');assert.equal(boot.version,'0.6.13');for(const browser of ['edge','chrome']){const state=await api('capture.bridge.status',{browser});if(state.removable)await api('capture.bridge.unregister',{browser});}
  await main.evaluate(d=>localStorage.setItem('shiqian-onboarding:v1:'+d,'done'),boot.dataPath);await main.reload();await main.locator('#workspace-sidebar').waitFor();
  assert.equal((await api('capture.bridge.status',{browser:'edge'})).registrationState,'missing');
  await api('capture.bridge.register',{browser:'edge'});
  assert.equal((await api('capture.bridge.status',{browser:'edge'})).registered,true);
  assert.equal((await api('capture.bridge.status')).registered,false); // old clients still mean Chrome
  await api('capture.bridge.register');
  await api('capture.bridge.unregister',{browser:'edge'});
  assert.equal((await api('capture.bridge.status')).registered,true);
  assert(fs.existsSync(path.join(qa,'bridge/host.json')));
  assert.equal((await api('capture.bridge.status')).enabled,true);
  await api('capture.bridge.register',{browser:'edge'});
  await api('capture.bridge.unregister',{browser:'chrome'});
  assert.equal((await api('capture.bridge.status',{browser:'edge'})).registered,true);
  await api('capture.bridge.register',{browser:'chrome'});
  const simulated=path.join(qa,'bridge/qa-registry-edge.json'),ownPointer=fs.readFileSync(simulated);
  fs.writeFileSync(simulated,JSON.stringify(path.join(qa,'foreign-host.json')));
  assert.equal((await api('capture.bridge.status',{browser:'edge'})).registrationState,'foreign');
  await assert.rejects(()=>api('capture.bridge.unregister',{browser:'edge'}));
  assert.equal((await api('capture.bridge.status',{browser:'chrome'})).registered,true);
  fs.writeFileSync(simulated,ownPointer);
  const tag=await api('tag.create',{name:'Edge 验收标签'});const tagId=tag.id||tag.tag?.id;assert(tagId);
  await main.getByRole('button',{name:'偏好设置',exact:true}).click();
  await main.getByRole('button',{name:'浏览器采集',exact:true}).click();
  await main.getByRole('combobox',{name:'选择采集浏览器',exact:true}).waitFor();
  await main.getByText('Edge 扩展连接',{exact:true}).waitFor();
  await main.screenshot({path:path.join(qa,'desktop-edge-settings.png'),animations:'disabled'});
  // Use an isolated QA host name so the user's production registry entries never change.
  const testExtension=path.join(qa,'edge-extension');fs.cpSync(path.join(root,'releases/v0.6.13/extensions/edge'),testExtension,{recursive:true});
  const worker=path.join(testExtension,'background.js');const original=fs.readFileSync(worker,'utf8');assert(original.includes('const HOST = "local.shiqian.capture";'));fs.writeFileSync(worker,original.replace('const HOST = "local.shiqian.capture";',`const HOST = "${host}";`));
  const hostManifest=JSON.parse(fs.readFileSync(path.join(qa,'bridge/native-host.json'),'utf8'));hostManifest.name=host;fs.writeFileSync(path.join(qa,'edge-test-native-host.json'),JSON.stringify(hostManifest));
  run('python','scripts/edge-native-test-host.py','register');hostRegistered=true;
  edge=await chromium.launchPersistentContext(path.join(qa,'edge-profile'),{executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true,viewport:{width:420,height:760},env:process.env,ignoreDefaultArgs:['--disable-extensions'],args:['--enable-unsafe-extension-debugging','--disable-background-networking',`--disable-extensions-except=${testExtension}`,`--load-extension=${testExtension}`]});
  const id='emppinjophbaookhjdigajhimbafcplg';
  try {const loaded=await(await edge.browser().newBrowserCDPSession()).send('Extensions.loadUnpacked',{path:testExtension});assert.equal(loaded.id,id);}catch(e){console.log('Checking command-line-loaded Edge extension after CDP load response:',String(e).split('\n')[0]);}
  popup=await edge.newPage();popup.on('pageerror',e=>errors.push(String(e)));await popup.goto(`chrome-extension://${id}/popup.html`);
  const status=await call('status');assert.equal(status.connected,true,JSON.stringify(status));assert.equal(status.version,'0.6.13');
  const catalog=await call('tags');assert(catalog.tags.some(t=>t.id===tagId));
  await popup.locator('#refresh-tags').click();await popup.getByRole('checkbox',{name:'Edge 验收标签',exact:true}).check();
  await popup.locator('#title').fill('Edge 网页验收');await popup.locator('#url').fill('https://example.com/tagrove-edge-page');await popup.locator('#submit').click();await popup.locator('#result').filter({hasText:/已保存到拾签|资料已存在/}).waitFor();
  await popup.screenshot({path:path.join(qa,'edge-capture-tags.png'),animations:'disabled'});
  const excerpt=await call('submit',{capture:{kind:'web_excerpt',title:'Edge 摘录验收',url:'https://example.com/tagrove-edge-excerpt',excerpt:'建筑外墙与学习资料：独立合成摘录。',tagIds:[tagId]}});assert.equal(excerpt.record.status,'done');assert.equal(excerpt.record.result.aiQueued,false);
  const repeated=native('capture',excerpt.record.capture);assert(repeated.ok);assert.equal(repeated.result.id,excerpt.record.result.id);assert.equal(repeated.result.duplicate,true);
  const image=await call('submit',{capture:{kind:'web_image',title:'Edge 图片验收',url:'https://github.com/SACO1F/Tagrove',imageUrl:'https://raw.githubusercontent.com/SACO1F/Tagrove/v0.6.12/public/icon.png',tagIds:[tagId]}});assert.equal(image.record.status,'done',JSON.stringify(image));
  const saved=await api('file',{id:image.record.result.id});assert.equal(saved.webSource.kind,'web_image');assert(saved.tags.some(t=>t.id===tagId));
  // Temporarily hide only the QA session to exercise an actual offline retry.
  const session=path.join(qa,'library/bridge-session.json'),hold=path.join(qa,'library/bridge-session.hold');fs.renameSync(session,hold);
  let queued;try{queued=await call('submit',{capture:{kind:'web_page',title:'Edge 离线验收',url:'https://example.com/tagrove-edge-offline',tagIds:[tagId]}});assert.equal(queued.record.status,'failed');assert.equal(queued.record.errorCode,'BRIDGE_OFFLINE');}finally{fs.renameSync(hold,session);}
  await call('retry',{id:queued.record.id});const queue=await call('list');assert.equal(queue.queue.find(r=>r.id===queued.record.id).status,'done');
  const before=(await api('query',{limit:100})).total;await call('retry',{id:queued.record.id});assert.equal((await api('query',{limit:100})).total,before);
  await popup.goto(`chrome-extension://${id}/queue.html`);await popup.locator('#queue').waitFor();await popup.screenshot({path:path.join(qa,'edge-queue.png'),animations:'disabled'});
  await api('capture.bridge.unregister',{browser:'chrome'});assert.equal((await api('capture.bridge.status',{browser:'edge'})).registered,true);await api('capture.bridge.unregister',{browser:'edge'});assert.equal((await api('capture.bridge.status',{browser:'edge'})).enabled,false);assert.equal(fs.existsSync(path.join(qa,'bridge/host.json')),false);
  assert.deepEqual(errors,[]);fs.writeFileSync(path.join(qa,'result.json'),JSON.stringify({exeSha256:crypto.createHash('sha256').update(fs.readFileSync(process.env.FLOATING_TEST_EXE)).digest('hex'),edgeVersion:await edge.browser().version(),version:'0.6.13',extensionId:id,independentRegistrations:true,lastRegistrationRemoval:true,foreignProtected:true,legacyChrome:true,realEdgeNative:true,web:true,excerpt:true,image:true,manualTags:true,offlineRetry:true,idempotence:true,productionRegistryUntouched:true,errors},null,2));
  console.log('PASS real Edge extension -> unique QA native host -> desktop: page/excerpt/image, selected tags, AI skip, offline queue/retry/idempotence; separate browser registrations, foreign protection and legacy Chrome API.');
}finally{
  if(edge)await edge.close();if(hostRegistered)run('python','scripts/edge-native-test-host.py','unregister');if(desktop)await desktop.close();run(process.execPath,'scripts/close-qa.cjs');
}})().catch(e=>{console.error(e);process.exitCode=1;});
