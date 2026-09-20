'use strict';
window.Graphics=(()=>{
 const pack=(r,g,b)=>(r<<16)|(g<<8)|b;
 function preset(name,size=128){const out=[];for(let iy=0;iy<size;iy++)for(let ix=0;ix<size;ix++){const x=ix*128/size,y=iy*128/size;
  let r,g,b;
  if(name==='planet'){
   r=18+Math.trunc(y*.25);g=14+Math.trunc(y*.1);b=45+Math.trunc(y*.3);
   const d=Math.hypot(x-76,y-48);if(d<29){r=240-Math.trunc(d*1.8);g=115+Math.trunc(y*.6);b=80+Math.trunc(d*2);}
   const ring=((x-73)+(y-50)*1.9)**2/58**2+((y-50)-(x-73)*.12)**2/9**2;
   if(ring>.8&&ring<1.2&&(y>47||d>29)){r=100;g=235;b=221;}
   const horizon=93+8*Math.sin(x*.09)+9*Math.sin(x*.23);
   if(y>horizon){r=20;g=43+Math.trunc((y-90)*.5);b=62;}if(y>horizon+12){r=9;g=23;b=35;}
  }else if(name==='bloom'){
   const dx=x-64,dy=y-64,rad=Math.hypot(dx,dy),angle=Math.atan2(dy,dx),edge=31+15*Math.cos(angle*7);
   r=15+Math.trunc(Math.max(0,42-rad)*.6);g=12;b=35+Math.trunc(Math.max(0,60-rad)*.8);
   if(rad<edge){r=150+Math.trunc(90*(1-rad/48));g=55+Math.trunc(95*(1-rad/48));b=170+Math.trunc(65*rad/48);}
   if(rad<edge&&rad>edge-3){r=255;g=157;b=218;}
   if(rad<10){r=255;g=212;b=118;}if(rad>53&&rad<55){r=80;g=183;b=190;}
  }else{
   r=35+Math.trunc(y*.28);g=18+Math.trunc(y*.08);b=68+Math.trunc(y*.23);
   if(Math.hypot(x-91,y-28)<17){r=246;g=176;b=104;}
   const heights=[85,65,93,42,78,54,88,69,94,49,81];const k=Math.min(10,Math.floor(x/12));
   if(y>heights[k]){r=12;g=29;b=46;if(x%12===1||x%12===2){r=75;g=179;b=184;}if(x%12>4&&x%12<8&&y%10>2&&y%10<5){r=237;g=124;b=163;}}
   if(y>112){r=21;g=34;b=48;}
  }
  out.push(pack(r,g,b));
 }return out;}
 function decode(text){const bytes=atob(text),out=[];for(let n=0;n<bytes.length;n+=3)out.push(pack(bytes.charCodeAt(n),bytes.charCodeAt(n+1),bytes.charCodeAt(n+2)));return out;}
 function draw(canvas,pixels){const size=Math.sqrt(pixels.length);if(canvas.width!==size){canvas.width=canvas.height=size;}const ctx=canvas.getContext('2d'),data=ctx.createImageData(size,size);for(let p=0;p<pixels.length;p++){const v=pixels[p];data.data[p*4]=v>>16;data.data[p*4+1]=(v>>8)&255;data.data[p*4+2]=v&255;data.data[p*4+3]=255;}ctx.putImageData(data,0,0);}
 function paint(canvas,s){const[cx,cy,rx,ry,kind,rotate,color,alpha]=s;const size=Math.sqrt(canvas.length),bx=rotate?rx+ry:rx,by=rotate?rx+ry:ry;for(let y=Math.max(0,cy-by);y<=Math.min(size-1,cy+by);y++)for(let x=Math.max(0,cx-bx);x<=Math.min(size-1,cx+bx);x++){const p=y*size+x;let dx=x-cx,dy=y-cy;if(rotate){const old=dx;dx+=dy;dy=old-dy;}dx=Math.abs(dx);dy=Math.abs(dy);const hit=kind===0?dx*dx*ry*ry+dy*dy*rx*rx<=rx*rx*ry*ry:kind===1?dx<=rx&&dy<=ry:dx*ry+dy*rx<=rx*ry;if(!hit)continue;let value=0;for(const shift of [0,8,16])value|=Math.floor((((canvas[p]>>shift)&255)*(255-alpha)+((color>>shift)&255)*alpha+127)/255)<<shift;canvas[p]=value;}}
 return{preset,decode,draw,paint};
})();
