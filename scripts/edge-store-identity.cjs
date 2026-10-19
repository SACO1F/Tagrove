// Exercise exact store/development origins against a real isolated desktop bridge.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {execFileSync}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=path.resolve(__dirname,'..'),qa=path.join(root,'qa/v0613-edge-store');
const dev='emppinjophbaookhjdigajhimbafcplg',store='oeeppgdhbgppkjgmjdaebffdhcfgeidi';
const run=(file,...args)=>execFileSync(file,args,{cwd:root,env:process.env,windowsHide:true,encoding:'utf8'});
function native(origin,action='ping',capture){const message=Buffer.from(JSON.stringify({protocol:1,action,capture})),header=Buffer.alloc(4);header.writeUInt32LE(message.length);const response=execFileSync(process.env.FLOATING_TEST_EXE,[origin],{cwd:root,env:process.env,windowsHide:true,input:Buffer.concat([header,message]),timeout:15000});return JSON.parse(response.subarray(4,4+response.readUInt32LE()));}
let desktop;
(async()=>{try{
  assert.equal(path.resolve(process.env.SHIQIAN_QA_DATA_DIR),path.join(qa,'library'));
  assert.equal(path.resolve(process.env.SHIQIAN_BRIDGE_CONFIG),path.join(qa,'bridge/host.json'));
  assert.equal(process.env.SHIQIAN_QA_BRIDGE_NO_REGISTRY,'1');
  fs.mkdirSync(qa,{recursive:true});run(process.execPath,'scripts/launch-qa.cjs',process.env.FLOATING_TEST_EXE);
  desktop=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.SHIQIAN_CDP_PORT);
  const main=desktop.contexts()[0].pages().find(p=>!p.url().includes('floating'));await main.locator('#workspace-sidebar').waitFor();
  const api=(action,payload={})=>main.evaluate(({action,payload})=>window.__TAURI_INTERNALS__.invoke('api',{action,payload}),{action,payload});
  const state=await api('capture.bridge.register',{browser:'edge'});assert.equal(state.extensionId,store);assert(state.registered);
  const config=path.join(qa,'bridge/host.json'),manifest=path.join(qa,'bridge/native-host.json');
  const expected=[`chrome-extension://${dev}/`,`chrome-extension://${store}/`];
  assert.deepEqual(JSON.parse(fs.readFileSync(manifest)).allowed_origins,expected);
  // Emulate a previous-version configuration without granting it the new origin.
  const legacy=JSON.parse(fs.readFileSync(config));delete legacy.extensionIds;fs.writeFileSync(config,JSON.stringify(legacy));
  const oldHost=JSON.parse(fs.readFileSync(manifest));oldHost.allowed_origins=[expected[0]];fs.writeFileSync(manifest,JSON.stringify(oldHost));
  const stale=await api('capture.bridge.status',{browser:'edge'});assert.equal(stale.registrationState,'stale');assert(stale.removable);
  assert.match(native(expected[1]).error,/BRIDGE_AUTH/);
  await api('capture.bridge.register',{browser:'edge'});
  for(const origin of expected){const reply=native(origin);assert(reply.ok&&reply.result.connected,JSON.stringify(reply));}
  for(const origin of ['chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/',`chrome-extension://${store}/extra`])assert.match(native(origin).error,/BRIDGE_AUTH/);
  const created=await api('tag.create',{name:'商店连接验收'}),tag=created.id||created.tag?.id;assert(tag);
  const capture={requestId:crypto.randomUUID(),kind:'web_page',title:'Edge 商店身份验收',url:'https://example.com/tagrove-edge-store-identity',tagIds:[tag]};
  const saved=native(expected[1],'capture',capture);assert(saved.ok,JSON.stringify(saved));assert.equal(saved.result.aiQueued,false);
  const repeated=native(expected[0],'capture',capture);assert(repeated.ok);assert.equal(repeated.result.id,saved.result.id);assert.equal(repeated.result.duplicate,true);
  await api('capture.bridge.register',{browser:'chrome'});await api('capture.bridge.unregister',{browser:'edge'});assert((await api('capture.bridge.status',{browser:'chrome'})).registered);
  await api('capture.bridge.unregister',{browser:'chrome'});assert(!fs.existsSync(config)&&!fs.existsSync(manifest));
  fs.writeFileSync(path.join(qa,'result.json'),JSON.stringify({exeSha256:crypto.createHash('sha256').update(fs.readFileSync(process.env.FLOATING_TEST_EXE)).digest('hex'),edgeStoreId:store,developerId:dev,legacyMigration:true,exactOriginEnforced:true,bothOriginsNativePing:true,storeOriginCapture:true,manualTagsSkipAi:true,crossOriginIdempotence:true,independentUnregister:true,productionRegistryUntouched:true,signedStoreInstallTested:false},null,2));
  console.log('PASS real native host: Edge store/development origins, legacy repair, unknown-origin rejection, tagged capture, shared idempotence, independent unregister. Store-signed installation not tested.');
}finally{if(desktop)await desktop.close();run(process.execPath,'scripts/close-qa.cjs');}})().catch(error=>{console.error(error);process.exitCode=1;});
