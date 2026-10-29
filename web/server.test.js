const {test}=require('node:test');
const assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const path=require('node:path');
const root=path.resolve(__dirname,'..');
const port=19000+Math.floor(Math.random()*10000), base=`http://localhost:${port}`;
function combo(a,b){const cards='23456789TJQKA'.split('').flatMap(r=>'cdhs'.split('').map(s=>r+s));let x=cards.indexOf(a),y=cards.indexOf(b);if(x>y)[x,y]=[y,x];return x*(103-x)/2+y-x-1}
const ranges=[Array(1326).fill(0),Array(1326).fill(0)];ranges[0][combo('As','Ah')]=1;ranges[0][combo('4s','4h')]=1;ranges[1][combo('Ks','Kh')]=1;
const good={board:['2c','3d','7h','9s','Jc'],pot:100,oopStack:100,ipStack:100,minBet:1,iterations:2000,delay:10,averaging:'linear',allIn:true,raiseCap:0,bets:[],raises:[],ranges};
const request=(url,body,headers={})=>fetch(base+url,{method:body?'POST':'GET',headers:{...(body?{'Content-Type':'application/json',Origin:base}:{}),...headers},body:body?JSON.stringify(body):undefined});
test('local API validates input and serves real polarized solver reports', {timeout:30000}, async()=>{
 const server=spawn(process.execPath,['web/server.js'],{cwd:root,env:{...process.env,PORT:String(port)}});
 try{await new Promise((resolve,reject)=>{server.stdout.once('data',resolve);server.once('error',reject);server.once('exit',code=>reject(Error(`server exited ${code}`)))});
 let r=await request('/api/solve',{...good,board:['2c','2c','7h','9s','Jc']});assert.equal(r.status,400);
 r=await request('/api/solve',{...good,iterations:5001});assert.equal(r.status,400);
 r=await request('/api/solve',good,{Origin:'http://evil.example'});assert.equal(r.status,403);
 r=await request('/api/solve',good);assert.equal(r.status,202);
 let s;for(let n=0;n<200;n++){await new Promise(resolve=>setTimeout(resolve,50));s=await (await request('/api/status')).json();if(s.phase!=='solving')break} assert.equal(s.phase,'ready',s.error);assert.equal(s.iterations,2000);assert.ok(Math.abs(s.result.ev[0]-25)<0.1);
 r=await request('/api/node?id=0');assert.equal(r.status,403);
 r=await request('/api/node?id=0',null,{'X-Poso-Request':'1'});assert.equal(r.status,200);const node=await r.json();assert.equal(node.actor,0);let bet=node.actions.findIndex(x=>x.label==='Bet to 100');assert.ok(bet>=0);assert.ok(node.reports[combo('As','Ah')].frequencies[bet]>.99);assert.ok(Math.abs(node.reports[combo('4s','4h')].frequencies[bet]-.5)<.02);assert.equal(node.reports[combo('2c','Ah')],null);
 const ip=await (await request(`/api/node?id=${node.actions[bet].child}`,null,{'X-Poso-Request':'1'})).json();assert.equal(ip.actor,1);const call=ip.actions.findIndex(x=>x.label==='Call');assert.ok(Math.abs(ip.reports[combo('Ks','Kh')].frequencies[call]-.5)<.02);
 }finally{server.kill('SIGINT')}
});

test('Rust worker independently rejects malformed and oversized inputs',()=>{
 const {spawnSync}=require('node:child_process');
 const binary=path.join(root,'target','release',process.platform==='win32'?'website_worker.exe':'website_worker');
 const call=lines=>spawnSync(binary,[],{input:lines.join('\n')+'\n',encoding:'utf8'});
 let result=call(['2c 2c 7h 9s Jc']);assert.notEqual(result.status,0);assert.match(result.stdout,/card appears more than once/i);
 result=call(['2c 3d 7h 9s Jc','100 100 100 1 5001 10 1 1 0']);assert.notEqual(result.status,0);assert.match(result.stdout,/outside allowed limits/);
 result=call(['2c 3d 7h 9s Jc','100 100 100 1 1 0 1 1 0','','','','1 2']);assert.notEqual(result.status,0);assert.match(result.stdout,/1,326 weights/);
});
