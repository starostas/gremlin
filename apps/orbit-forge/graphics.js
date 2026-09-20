'use strict';
window.Orbit=(()=>{
 const Q=268435456n,P=843314857n,mul=(a,b)=>(a*b)>>28n;
 const sin=[268435456n,-44739243n,2236962n,-53261n,740n,-7n],cos=[268435456n,-134217728n,11184811n,-372827n,6658n,-74n];
 const clamp=(x,a,b)=>x<a?a:x>b?b:x;
 function solve(g,m,e){m=BigInt(m);e=BigInt(e);let x=[()=>m,()=>P/2n,()=>m+e/2n,()=>m+e,()=>m<Q?m+e:m+e/4n][g.seed]();x=clamp(x,0n,P);let lo=0n,hi=P;
  for(let tick=0;tick<g.iterations;tick++){let y=x>P/2n?P-x:x,z=mul(y,y);const poly=c=>{let p=c.at(-1);for(let i=c.length-2;i>=0;i--)p=c[i]+mul(p,z);return p;};let sn=mul(y,poly(sin)),cs=poly(cos)*(x>P/2n?-1n:1n),es=mul(e,sn),f=x-m-es;
   if((f<0n?-f:f)<[0n,16n,256n,4096n][g.stop])break;if(f>0n)hi=x;else lo=x;let d=Q-mul(e,cs),op=tick<g.switch?g.first:g.later,step;if(op===0)step=f*Q/d;else if(op===1){let n=f*Q/d,h=d-mul(es,n)/2n;step=h>26843n?f*Q/h:n;}else if(op===2)step=(f*Q/d)/2n;else step=f;if(g.cap){let cap=g.cap===1?Q:Q/2n;step=clamp(step,-cap,cap);}let next=x-step;x=g.guard&&(next<lo||next>hi)?(lo+hi)/2n:clamp(next,0n,P);
  }return Number(x);
 }
 function truth(m,e){let a=0,b=Math.PI+1e-8;for(let k=0;k<55;k++){let x=(a+b)/2;if(x-e*Math.sin(x)>m)b=x;else a=x;}return(a+b)/2;}
 function draw(canvas,g,e,phase){const c=canvas.getContext('2d'),w=canvas.width,h=canvas.height;c.fillStyle='#080e1b';c.fillRect(0,0,w,h);for(let n=0;n<95;n++){c.fillStyle=n%4?'#22334b':'#517295';c.fillRect((n*173+31)%w,(n*89+53)%h,1.5,1.5);}const scale=w*.29,cx=w/2+e*scale*.45,cy=h/2;const point=E=>[cx+scale*(Math.cos(E)-e),cy+scale*Math.sqrt(1-e*e)*Math.sin(E)];c.beginPath();for(let k=0;k<=360;k++){let[x,y]=point(k*Math.PI/180);if(!k)c.moveTo(x,y);else c.lineTo(x,y);}c.strokeStyle='#38516b';c.lineWidth=1.5;c.stroke();c.fillStyle='#ffd899';c.shadowColor='#ffc16b';c.shadowBlur=22;c.beginPath();c.arc(cx,cy,7,0,Math.PI*2);c.fill();c.shadowBlur=0;
  const sign=phase>Math.PI?-1:1,m=sign<0?2*Math.PI-phase:phase,mi=Math.round(m*Number(Q)),ei=Math.round(e*Number(Q)),ref=truth(mi/Number(Q),ei/Number(Q)),actual=g?solve(g,mi,ei)/Number(Q):ref;
  for(let n=0;n<30;n++){let p=phase-n*.025;p=(p+Math.PI*2)%(Math.PI*2);let s=p>Math.PI?-1:1,mm=s<0?Math.PI*2-p:p;let E=g?solve(g,Math.round(mm*Number(Q)),ei)/Number(Q):truth(mm,e);let[x,y]=point(s*E);c.fillStyle=`rgba(91,231,204,${.4*(1-n/30)})`;c.beginPath();c.arc(x,y,2,0,Math.PI*2);c.fill();}
  let[rx,ry]=point(sign*ref),[x,y]=point(sign*actual);c.strokeStyle='#f1f6ff';c.lineWidth=2;c.beginPath();c.arc(rx,ry,10,0,Math.PI*2);c.stroke();c.fillStyle='#6df0d1';c.beginPath();c.arc(x,y,6,0,Math.PI*2);c.fill();return {m,error:Math.abs(actual-ref),actual};
 }
 function heat(canvas,grid,tolerance){const c=canvas.getContext('2d'),im=c.createImageData(256,256);for(let e=0;e<256;e++)for(let m=0;m<256;m++){const v=grid[e*256+m],t=Math.min(1,Math.max(0,(Math.log10(Math.max(v,1e-10))-Math.log10(tolerance)+5)/5)),p=((255-e)*256+m)*4;im.data[p]=v>tolerance?255:Math.round(20+170*t);im.data[p+1]=v>tolerance?71:Math.round(75+100*(1-t));im.data[p+2]=v>tolerance?85:Math.round(100+115*t);im.data[p+3]=255;}c.putImageData(im,0,0);}
 return{solve,truth,draw,heat};
})();
