'use strict';
window.Robot=(()=>{
 const neighbor=(p,d)=>[p-8,p+1,p+8,p-1][d&3]&63;
 const inside=p=>(p>>3)>0&&(p>>3)<7&&(p&7)>0&&(p&7)<7;
 const neighbors=p=>[0,1,2,3].map(d=>neighbor(p,d)).filter(inside);
 const wall=(r,p)=>(BigInt('0x'+r.walls)>>BigInt(p)&1n)!==0n;
 function sensors(r,p,d,key){const free=n=>!wall(r,n)&&(key||n!==r.door);return [free(neighbor(p,d)),free(neighbor(p,(d+3)&3)),free(neighbor(p,(d+1)&3))];}
 function simulate(decisions,r){
  let p=r.start,d=r.heading,memory=0,key=false,steps=0;const seen=new Set(),frames=[];
  while(true){seen.add(p);if(p===r.key)key=true;frames.push([p,d,memory,key]);if((p===r.exit&&key)||steps===256)break;
   const [front,left,right]=sensors(r,p,d,key),row=Number(front)+2*Number(left)+4*Number(right)+8*memory;
   const decision=decisions[row];memory=decision>>2;
   switch(decision&3){case 0:if(front)p=neighbor(p,d);break;case 1:d=(d+3)&3;break;case 2:d=(d+1)&3;break;case 3:d=(d+2)&3;break;}steps++;
  }
  return {solved:p===r.exit&&key,key,steps,visited:seen.size,frames};
 }
 function distances(r,start,locked=false){const out=Array(64).fill(999),queue=[start];out[start]=0;for(let i=0;i<queue.length;i++){const p=queue[i];for(const n of neighbors(p)){if(!wall(r,n)&&!(locked&&n===r.door)&&out[n]===999){out[n]=out[p]+1;queue.push(n);}}}return out;}
 function validate(r){
  const items=[r.start,r.exit,r.key,r.door];
  if(new Set(items).size!==4||items.some(p=>!inside(p)||wall(r,p)))return 'Place start, key, door, and exit on four different floor tiles.';
  if(distances(r,r.start,true)[r.key]===999)return 'The key is unreachable while the door is locked.';
  if(distances(r,r.key)[r.exit]===999)return 'There is no path from the key to the exit.';
  return null;
 }
 function fresh(){
  const pick=a=>a[Math.floor(Math.random()*a.length)];
  for(let attempt=0;attempt<100;attempt++){
   const floors=new Set([9+Math.floor(Math.random()*6)+8*Math.floor(Math.random()*6)]);
   while(true){const choices=Array.from({length:64},(_,i)=>i).filter(p=>inside(p)&&!floors.has(p)&&neighbors(p).filter(n=>floors.has(n)).length===1);if(!choices.length)break;floors.add(pick(choices));}
   if(floors.size<15)continue;
   let bits=(1n<<64n)-1n;for(const p of floors)bits&=~(1n<<BigInt(p));
   const cells=[...floors],start=pick(cells),r={walls:bits.toString(16).padStart(16,'0'),start,heading:Math.floor(Math.random()*4),exit:0,key:0,door:0};
   const ds=distances(r,start);r.exit=cells.reduce((a,b)=>ds[a]>ds[b]?a:b);
   const path=[r.exit];let at=r.exit;while(at!==start){at=neighbors(at).find(p=>ds[p]+1===ds[at]);path.push(at);}
   if(path.length<5)continue;r.door=path[Math.floor(path.length/2)];
   const reach=distances(r,start,true),keys=cells.filter(p=>reach[p]!==999&&![start,r.exit,r.door].includes(p));if(!keys.length)continue;r.key=pick(keys);return r;
  }
  throw Error('Could not generate a room');
 }
 return {neighbor,inside,wall,sensors,simulate,validate,fresh};
})();
