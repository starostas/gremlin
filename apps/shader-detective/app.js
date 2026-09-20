'use strict';
const $=id=>document.getElementById(id);
let sample=null,active=false,replaying=false,cursor=0,epoch=0,results={},curve=[],diagnostic='',caps={};
const number=n=>new Intl.NumberFormat('en',{notation:'compact',maximumFractionDigits:1}).format(n);
function paint(id,pixels){
  if(!pixels)return;
  const canvas=$(id),ctx=canvas.getContext('2d'),data=ctx.createImageData(canvas.width,canvas.height);
  if(pixels.image){
    const raw=atob(sample.images[pixels.image]);
    for(let i=0;i<raw.length/3;i++){data.data[i*4]=raw.charCodeAt(i*3);data.data[i*4+1]=raw.charCodeAt(i*3+1);data.data[i*4+2]=raw.charCodeAt(i*3+2);data.data[i*4+3]=255;}
  }else{
    for(let i=0;i<pixels.length;i++){data.data[i*4]=(pixels[i]>>>16)&255;data.data[i*4+1]=(pixels[i]>>>8)&255;data.data[i*4+2]=pixels[i]&255;data.data[i*4+3]=255;}
  }
  ctx.putImageData(data,0,0);
}
function chart(){
  const canvas=$('chart'),ctx=canvas.getContext('2d'),w=canvas.width,h=canvas.height;
  ctx.clearRect(0,0,w,h);ctx.strokeStyle='#2a3540';ctx.lineWidth=1;
  for(let y=12;y<h;y+=24){ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(w,y);ctx.stroke();}
  if(!curve.length)return;
  const max=Math.max(...curve.map(p=>p[1]),.001);
  ctx.strokeStyle='#bbf575';ctx.lineWidth=2;ctx.beginPath();
  curve.forEach(([generation,error],i)=>{const x=5+(generation-1)/79*(w-10),y=h-8-error/max*(h-20);if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y);});ctx.stroke();
}
function busy(value){
  active=value;
  for(const id of ['preset','mode','cases','seed','replay'])$(id).disabled=value;
  $('run').disabled=value||!caps.live;
  $('replay').disabled=value||!sample;
  if(!value&&!caps.gpu)$('mode').value='cpu';
  $('stop').hidden=!value;
}
function reset(){
  results={};curve=[];diagnostic='';cursor=0;
  $('validation-title').textContent='Earn the green light.';$('validation-title').className='';
  $('validation-copy').textContent='The best program must match the training set, fresh colors, and every pixel in the preview.';
  $('comparison').hidden=true;$('match').textContent='';$('program').textContent='// Waiting for a candidate…';
  for(const id of ['generation','accuracy','elapsed','evaluations'])$(id).textContent='—';
  chart();
}
function accept(event){
  if(event.kind==='start'){
    curve=[];chart();$('mode').value=results.cpu?'both':(replaying?'both':$('mode').value);
    $('preset').value=event.preset;$('cases').value=event.cases;$('seed').value=event.seed;
    for(const id of ['input','target','preview']){$(id).width=event.width;$(id).height=event.height;}
    paint('input',event.input);paint('target',event.target);paint('preview',event.input);
    $('status').textContent=`Searching on ${event.mode.toUpperCase()}…`;$('run-badge').textContent=replaying?'RECORDED RUN':'LIVE SEARCH';
    $('notice').textContent=replaying?`Recorded ${sample.hardware} comparison. Playback is accelerated; times below are the measured search times.`:`${number(event.cases)} training colors · 256 candidates · 80-generation ceiling. ${event.mode==='gpu'?'One CUDA context for this run.':'CPU reference interpreter.'}`;
    $('match').textContent='';$('validation-title').className='';$('validation-title').textContent='Looking for an exact match.';
    $('validation-copy').textContent='Fresh observations are being checked. Independent validation follows discovery.';
    $('program').textContent='// Evaluating the initial population…';
    for(const id of ['generation','accuracy','elapsed','evaluations'])$(id).textContent='—';
  }else if(event.kind==='progress'){
    $('generation').textContent=event.generation;$('accuracy').textContent=((1-event.mismatches/event.cases)*100).toFixed(1)+'%';
    $('elapsed').textContent=event.elapsed_seconds.toFixed(2)+'s';$('evaluations').textContent=number(event.evaluations);
    if(event.program)$('program').textContent=event.program;paint('preview',event.preview);
    curve.push([event.generation,event.bit_errors/(event.cases*32)]);chart();
  }else if(event.kind==='done'){
    results[event.mode]=event;paint('preview',event.preview);$('program').textContent=event.program;
    $('elapsed').textContent=event.search_seconds.toFixed(2)+'s';$('generation').textContent=event.generation;
    $('status').textContent=event.success?'The shader has been recovered.':'This run did not pass validation.';
    $('match').textContent=event.image_matches?'MATCH':'DIFFERS';$('run-badge').textContent=replaying?'RECORDED RESULT':(event.success?'VALIDATED':'NOT VALIDATED');
    $('validation-title').textContent=event.success?'4,096 fresh colors. Zero surprises.':'Validation found a mismatch.';
    $('validation-title').className=event.success?'good':'bad';
    $('validation-copy').textContent=`${event.holdout_mismatches} mismatches on ${event.holdout_cases.toLocaleString()} unseen colors. ${event.image_matches?'Every preview pixel matches.':'Preview differs.'} Tested, not formally proved.`;
    if(results.cpu&&results.gpu){
      const same=results.cpu.search_state_hash===results.gpu.search_state_hash;
      $('comparison').hidden=false;
      $('comparison').textContent=same?`Identical search · CPU ${results.cpu.search_seconds.toFixed(2)}s / GPU ${results.gpu.search_seconds.toFixed(2)}s · ${(results.cpu.search_seconds/results.gpu.search_seconds).toFixed(2)}× GPU speedup`:'CPU and GPU search states differ; comparison is not validated.';
      $('comparison').className=same?'good':'bad';
    }
  }else if(event.kind==='diagnostic'){
    diagnostic=event.message;$('notice').textContent=diagnostic;
  }else if(event.kind==='error'){
    $('status').textContent='Run could not complete.';$('notice').textContent=event.message+(diagnostic?' '+diagnostic:'');$('run-badge').textContent='ERROR';
    $('validation-title').textContent='No result claimed.';$('validation-title').className='bad';
  }else if(event.kind==='finished'){
    busy(false);
  }
}
async function api(path,body){
  const response=await fetch(path,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await response.json();if(!response.ok)throw new Error(data.error||`HTTP ${response.status}`);return data;
}
async function poll(token){
  if(token!==epoch||replaying)return;
  try{
    const data=await api('/api/events?after='+cursor);
    for(const event of data.events){cursor=event.id;accept(event);}
    if(!data.running){busy(false);return;}
  }catch(error){$('notice').textContent=error.message;busy(false);return;}
  setTimeout(()=>poll(token),400);
}
$('run').onclick=async()=>{
  const settings={mode:$('mode').value,preset:$('preset').value,cases:Number($('cases').value),seed:Number($('seed').value)};
  const token=++epoch;replaying=false;reset();busy(true);$('status').textContent='Preparing observations…';$('run-badge').textContent='STARTING';
  try{await api('/api/run',settings);poll(token);}catch(error){accept({kind:'error',message:error.message});busy(false);}
};
$('stop').onclick=async()=>{
  if(replaying){++epoch;replaying=false;busy(false);$('status').textContent='Replay stopped.';return;}
  try{await api('/api/cancel',{});$('notice').textContent='Stopping the active run…';}catch(error){$('notice').textContent=error.message;}
};
$('replay').onclick=async()=>{
  if(!sample)return;
  const token=++epoch;reset();replaying=true;busy(true);$('mode').value='both';
  for(const event of sample.events){
    if(token!==epoch)return;
    accept(event);await new Promise(resolve=>setTimeout(resolve,event.kind==='done'?700:55));
  }
  if(token===epoch){replaying=false;busy(false);$('run-badge').textContent='RECORDED RESULT';}
};
(async()=>{
  try{
    const state=await api('/api/state');caps=state.capabilities;
    $('connection').textContent=caps.gpu?(caps.remote?'REMOTE GPU CONFIGURED':'LOCAL GPU CONFIGURED'):'CPU / REPLAY';
    for(const option of $('mode').options)if(option.value!=='cpu')option.disabled=!caps.gpu;
    $('mode').value=caps.gpu?'both':'cpu';busy(false);
    try{
      const response=await fetch('/sample.json');if(response.ok){sample=await response.json();
        const first=sample.events.find(e=>e.kind==='start');
        for(const id of ['input','target','preview']){$(id).width=first.width;$(id).height=first.height;}
        paint('input',first.input);paint('target',first.target);paint('preview',first.input);
      }
    }catch(error){$('notice').textContent='Recording unavailable. Live runs still work.';}
    $('replay').disabled=!sample;
    if(state.running){reset();busy(true);poll(++epoch);}
  }catch(error){$('notice').textContent=error.message;$('run').disabled=true;$('connection').textContent='SERVER UNAVAILABLE';}
})();
