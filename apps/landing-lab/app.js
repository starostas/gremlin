'use strict';
const $=id=>document.getElementById(id);
const n=v=>Number(v).toLocaleString('en-US');
let epoch=0, busy=false, recording=false, capabilities={}, results={}, bestId=null;
const views={baseline:{traces:[],start:performance.now()},best:{traces:[],start:performance.now()}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
function state(active){busy=active; for(const id of ['run','replay','mode','cases','seed']) $(id).disabled=active || (id==='run'&&!capabilities.live); $('stop').disabled=!active;}
function reset(){results={};bestId=null;$('tested').textContent='0 / 128';$('safe').textContent='—';$('elapsed').textContent='—';$('speedup').textContent='—';$('results').textContent='Every returned outcome and step count is included in the CPU/GPU comparison.';$('validation').textContent='NOT YET TESTED';}
function traces(name,data,changed=true){views[name].traces=data;if(changed)views[name].start=performance.now();}
function processEvent(e){
 const prefix=recording?'RECORDING · ':'';
 if(e.kind==='start'){
  $('status').textContent=prefix+e.mode.toUpperCase()+' SEARCH';
  $('detail').textContent=`128 programs × ${n(e.cases)} flights · ≤256 ticks per flight`;
  traces('baseline',e.baseline);traces('best',e.baseline);bestId=null;
 }
 if(e.kind==='progress'){
  $('tested').textContent=`${e.tested} / 128`;$('safe').textContent=`${n(e.safe)} / ${n(e.cases)}`;$('elapsed').textContent=e.seconds.toFixed(2)+' s';
  $('baseline-rate').textContent=(100*e.baseline_safe/e.cases).toFixed(1)+'% SAFE';$('best-rate').textContent=(100*e.safe/e.cases).toFixed(1)+'% SAFE';
  $('policy').textContent=e.description;$('source').textContent=e.source;traces('best',e.traces,bestId!==e.best);bestId=e.best;
 }
 if(e.kind==='done'){
  results[e.mode]=e;$('elapsed').textContent=e.seconds.toFixed(2)+' s';$('validation').textContent=`HOLDOUT ${n(e.holdout_safe)} / ${n(e.holdout_cases)}`;
  const box=$('results');box.replaceChildren();
  for(const r of Object.values(results)){
   const p=document.createElement('p');p.textContent=`${recording?'Recorded ':''}${r.mode.toUpperCase()}: ${r.seconds.toFixed(2)} s search / ${r.total_seconds.toFixed(2)} s total. ${n(r.safe)} of ${n(r.cases)} training flights landed safely.`;box.append(p);
  }
  const p=document.createElement('p');p.textContent=`${n(e.executed_steps)} interpreted instructions. ${e.mode==='gpu'?(e.peak_bytes/1048576).toFixed(0)+' MiB peak device allocation.':e.device+'.'}`;box.append(p);
  $('status').textContent=prefix+e.mode.toUpperCase()+' COMPLETE';
 }
 if(e.kind==='comparison'){
  if(!e.matching)throw Error('CPU/GPU parity failed');
  $('speedup').textContent=e.speedup.toFixed(1)+'×';
  const p=document.createElement('p');p.textContent=`Exact CPU/GPU outcome + step-count parity. ${e.total_speedup.toFixed(1)}× faster including setup and holdout validation.`;$('results').append(p);
  $('status').textContent=prefix+'COMPARISON VERIFIED';
 }
 if(e.kind==='error'){ $('status').textContent='ERROR · '+e.message; }
 if(e.kind==='finished'){state(false);}
}
async function request(path,body){const response=await fetch(path,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});const data=await response.json();if(!response.ok)throw Error(data.error||response.statusText);return data;}
async function poll(id){let after=0;while(id===epoch){const data=await request('/api/events?after='+after);if(id!==epoch)return;for(const e of data.events){processEvent(e);after=e.id;}if(!data.running){state(false);return;}await sleep(350);}}
function fail(e){$('status').textContent='ERROR · '+e.message;state(false);}
$('run').onclick=async()=>{const id=++epoch;recording=false;reset();state(true);$('status').textContent='STARTING ENGINE';try{await request('/api/run',{mode:$('mode').value,preset:'lander',cases:Number($('cases').value),seed:Number($('seed').value)});await poll(id);}catch(e){fail(e);}};
$('stop').onclick=async()=>{if(recording){++epoch;state(false);$('status').textContent='RECORDING STOPPED';}else{try{await request('/api/cancel',{});$('status').textContent='STOPPING ENGINE';}catch(e){fail(e);}}};
$('replay').onclick=async()=>{const id=++epoch;recording=true;reset();state(true);$('status').textContent='LOADING RECORDING';try{const data=await request('/sample.json');for(const recorded of data.events){if(id!==epoch)return;const e={...recorded};for(const key of ['traces','baseline'])if(e[key]?.trace)e[key]=data.traces[e[key].trace];processEvent(e);await sleep(e.kind==='progress'?420:500);}if(id===epoch)state(false);}catch(e){fail(e);}};
function draw(name,now){
 const canvas=$(name),ctx=canvas.getContext('2d'),W=canvas.width,H=canvas.height,v=views[name];
 const X=x=>W/2+x*W/1900,Y=y=>H-54-Math.max(0,y)*(H-92)/7000;
 ctx.fillStyle='#0a111b';ctx.fillRect(0,0,W,H);
 for(let i=0;i<80;i++){ctx.fillStyle=i%3?'#263d50':'#647989';ctx.fillRect((i*137+17)%W,(i*71+11)%(H-95),i%7?1:2,1);}
 ctx.strokeStyle='#122333';ctx.lineWidth=1;
 for(let y=1000;y<=6000;y+=1000){ctx.beginPath();ctx.moveTo(0,Y(y));ctx.lineTo(W,Y(y));ctx.stroke();ctx.fillStyle='#3e5364';ctx.font='9px monospace';ctx.fillText((y/100)+' m',10,Y(y)-6);}
 ctx.fillStyle='#1b2836';ctx.beginPath();ctx.moveTo(0,H);for(let x=0;x<=W;x+=18)ctx.lineTo(x,H-45-Math.sin(x*.031)*6-Math.sin(x*.083)*4);ctx.lineTo(W,H);ctx.fill();
 ctx.fillStyle='#a9f5c9';ctx.fillRect(X(-48),H-54,X(48)-X(-48),3);ctx.fillStyle='#758d9b';ctx.font='9px monospace';ctx.textAlign='center';ctx.fillText('LANDING PAD',W/2,H-17);ctx.textAlign='left';
 const t=Math.max(0,Math.floor((now-v.start)/52))%290;
 for(let j=0;j<v.traces.length;j++){
  const trace=v.traces[j],frames=trace.frames,f=frames[Math.min(t,frames.length-1)],done=t>=frames.length-1;
  ctx.strokeStyle=trace.safe?'#a9f5c91c':'#ff8e7d15';ctx.beginPath();for(let k=0;k<Math.min(t+1,frames.length);k+=3){const a=frames[k];if(k===0)ctx.moveTo(X(a[0]),Y(a[1]));else ctx.lineTo(X(a[0]),Y(a[1]));}ctx.stroke();
  const x=X(f[0]),y=Y(f[1]);if(x<0||x>W)continue;
  ctx.fillStyle=done?(trace.safe?'#a9f5c9':'#fb8379'):'#a8c6e0';
  if(done&&!trace.safe){ctx.beginPath();ctx.moveTo(x-3,y-3);ctx.lineTo(x+3,y+3);ctx.moveTo(x+3,y-3);ctx.lineTo(x-3,y+3);ctx.strokeStyle='#fb8379';ctx.stroke();}
  else {ctx.fillRect(x-2,y-2,4,4);if(!done&&f[5]){ctx.fillStyle='#ffb16c';ctx.fillRect(x-1,y+3,2,4+(t+j)%4);}}
 }
 ctx.fillStyle='#718b9c';ctx.font='10px monospace';ctx.fillText(v.traces.length?'48 FLIGHTS · SIMULATION PLAYBACK':'WAITING FOR FLIGHT DATA',16,24);
}
function animate(now){draw('baseline',now);draw('best',now);requestAnimationFrame(animate);}requestAnimationFrame(animate);
(async()=>{try{const s=await request('/api/state');capabilities=s.capabilities;$('connection').textContent=capabilities.gpu?(capabilities.remote?'REMOTE GPU CONFIGURED':'LOCAL GPU CONFIGURED'):(capabilities.live?'CPU ENGINE READY':'RECORDED DEMO AVAILABLE');for(const option of $('mode').options){if(option.value!=='cpu')option.disabled=!capabilities.gpu;}if(!capabilities.gpu)$('mode').value='cpu';state(s.running);if(s.running)await poll(++epoch);}catch(e){fail(e);}})();
