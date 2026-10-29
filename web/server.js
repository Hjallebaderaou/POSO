'use strict';
// Local-only server: no dependencies, no remote execution service.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const root = path.resolve(__dirname, '..');
const port = Number(process.env.PORT || 3000);
if (!Number.isInteger(port) || port < 1024 || port > 65535) throw Error('PORT must be 1024–65535');
let job = null;
const send = (res, status, data) => { if (!res.writableEnded) { res.writeHead(status, {'Content-Type':'application/json; charset=utf-8','Cache-Control':'no-store','X-Content-Type-Options':'nosniff'}); res.end(JSON.stringify(data)); } };
const stop = () => { if (job?.timer) clearTimeout(job.timer); if (job?.worker && !job.worker.killed) job.worker.kill(); };
function validInput(x) {
  const integer = (n,min,max) => Number.isInteger(n) && n>=min && n<=max;
  if (!x || !Array.isArray(x.board) || x.board.length!==5 || x.board.some(c=>typeof c!=='string'||! /^[2-9TJQKA][cdhs]$/.test(c)) || new Set(x.board).size!==5) throw Error('Board must contain five unique cards.');
  for (const k of ['pot','oopStack','ipStack','minBet']) if (!integer(x[k],1,1000000)) throw Error(`${k} must be an integer from 1 to 1,000,000.`);
  if (!integer(x.iterations,1,5000) || !integer(x.delay,0,x.iterations-1)) throw Error('Iterations must be 1–5000, and averaging delay must be smaller.');
  if (!['linear','uniform'].includes(x.averaging) || typeof x.allIn!=='boolean' || !integer(x.raiseCap,0,2)) throw Error('Invalid solver settings.');
  if (!Array.isArray(x.bets) || !Array.isArray(x.raises) || x.bets.length>3 || x.raises.length>3 || x.bets.some(n=>!integer(n,1,500)) || x.raises.some(r=>!r || !integer(r.size,1,500) || !['after','current'].includes(r.basis))) throw Error('Use up to three sizes between 1% and 500%.');
  if (!Array.isArray(x.ranges) || x.ranges.length!==2 || x.ranges.some(r=>!Array.isArray(r)||r.length!==1326||r.some(n=>typeof n!=='number'||!Number.isFinite(n)||n<0||n>1))) throw Error('Ranges require 1,326 weights between 0 and 1.');
}
function start(x) {
  const worker=spawn(path.join(root,'target','release',process.platform==='win32'?'website_worker.exe':'website_worker'),[],{stdio:['pipe','pipe','pipe']});
  const current={worker, phase:'solving', iterations:0, total:x.iterations, pending:null}; job=current;
  const input=[x.board.join(' '),[x.pot,x.oopStack,x.ipStack,x.minBet,x.iterations,x.delay,x.averaging==='linear'?1:0,x.allIn?1:0,x.raiseCap].join(' '),x.bets.join(' '),x.raises.map(r=>r.size).join(' '),x.raises.map(r=>r.basis==='after'?0:1).join(' '),...x.ranges.map(r=>r.join(' '))].join('\n')+'\n';
  worker.stdin.on('error',()=>{}); worker.stdin.write(input);
  current.timer=setTimeout(()=>{current.phase='error';current.error='Solve timed out after 120 seconds. Reduce the workload.';worker.kill();},120000);
  readline.createInterface({input:worker.stdout}).on('line',line=>{let msg;try{msg=JSON.parse(line)}catch{return} if(msg.type==='progress') current.iterations=msg.iterations; else if(msg.type==='ready'){clearTimeout(current.timer);current.phase='ready';current.result=msg;} else if(msg.type==='error'){clearTimeout(current.timer);current.phase='error';current.error=msg.message; if(current.pending){send(current.pending,400,{error:msg.message});current.pending=null;}} else if(msg.type==='node'||msg.type==='terminal'){if(current.pending){send(current.pending,200,msg);current.pending=null;}}});
  worker.on('error',()=>{current.phase='error';current.error='Worker unavailable. Run cargo build --release --bin website_worker first.';});
  worker.on('exit',()=>{clearTimeout(current.timer);if(current.phase!=='error') {current.phase='error';current.error='Solver worker stopped.';} if(current.pending){send(current.pending,500,{error:current.error});current.pending=null;}});
  worker.stderr.resume();
}
const server=http.createServer((req,res)=>{
  const host=req.headers.host; if(host!==`localhost:${port}` && host!==`127.0.0.1:${port}`){send(res,403,{error:'Invalid host'});return;}
  if(req.headers.origin && req.headers.origin!==`http://${host}`){send(res,403,{error:'Invalid origin'});return;}
  const url=new URL(req.url,`http://${host}`);
  if(req.method==='GET' && ['/','/app.js','/matrix.js','/style.css'].includes(url.pathname)) {const file=url.pathname==='/'?'index.html':url.pathname.slice(1);res.writeHead(200,{'Content-Type':file.endsWith('.js')?'text/javascript; charset=utf-8':file.endsWith('.css')?'text/css; charset=utf-8':'text/html; charset=utf-8','Cache-Control':'no-store','X-Content-Type-Options':'nosniff','Content-Security-Policy':"default-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"});fs.createReadStream(path.join(__dirname,file)).pipe(res);return;}
  if(req.method==='POST' && url.pathname==='/api/solve') {if(!req.headers.origin || req.headers['content-type']!=='application/json'){send(res,403,{error:'Same-origin JSON required'});return} if(job?.phase==='solving'){send(res,409,{error:'A solve is already running.'});return} let chunks=[],size=0;req.on('data',c=>{size+=c.length;if(size>100000){send(res,413,{error:'Input too large'});req.destroy();} else chunks.push(c)});req.on('end',()=>{if(size>100000)return;try{const x=JSON.parse(Buffer.concat(chunks));validInput(x);stop();start(x);send(res,202,{ok:true})}catch(e){send(res,400,{error:e.message})}});return;}
  if(req.method==='GET' && url.pathname==='/api/status'){send(res,200,job?{phase:job.phase,iterations:job.iterations,total:job.total,result:job.result,error:job.error}:{phase:'idle'});return;}
  if(req.method==='GET' && url.pathname==='/api/node'){if(req.headers['x-poso-request']!=='1'){send(res,403,{error:'Same-origin request required'});return} const raw=url.searchParams.get('id');if(!job||job.phase!=='ready'){send(res,409,{error:'No completed solve'});return} if(job.pending){send(res,429,{error:'A node request is already running'});return} if(!/^\d{1,3}$/.test(raw||'')||Number(raw)>=job.result.nodes){send(res,400,{error:'Invalid node ID'});return} job.pending=res;job.worker.stdin.write(`node ${Number(raw)}\n`);return;}
  send(res,404,{error:'Not found'});
});
server.listen(port,'127.0.0.1',()=>console.log(`POSO is running at http://localhost:${port} (local only). Press Ctrl+C to stop.`));
process.on('SIGINT',()=>{stop();server.close(()=>process.exit(0));});
