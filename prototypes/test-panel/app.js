'use strict';
const $=id=>document.getElementById(id), ROW=224;
let cases=[{input:'21\n',expected:'42\n',actual:'',compare:true,state:'未运行'},{input:'0\n',expected:'0\n',actual:'',compare:true,state:'未运行'}];
if(new URLSearchParams(location.search).has('empty'))cases=[];
let messagesResized=false;let hasOutput=false;
let range='',created=0;
function render(force=false){
 const first=Math.max(0,Math.floor($('cases').scrollTop/ROW)-1),last=Math.min(cases.length,Math.ceil(($('cases').scrollTop+$('cases').clientHeight)/ROW)+1),next=`${first}:${last}`;
 $('count').textContent=cases.length;
 $('empty-tests').hidden=cases.length!==0;$('add').hidden=cases.length===0;
 if(!messagesResized)document.documentElement.style.setProperty('--messages',hasOutput?'150px':'76px');
 $('spacer').style.height=`${cases.length*ROW}px`;
 if(!force&&range===next)return;
 range=next;$('visible').replaceChildren();$('visible').style.transform=`translateY(${first*ROW}px)`;
 for(let i=first;i<last;i++){
  const c=cases[i],row=document.createElement('section');row.className='case';row.setAttribute('aria-label',`用例 ${i+1}`);
  const head=document.createElement('div');head.className='case-head';const title=document.createElement('div');const name=document.createElement('strong');name.textContent=`用例 ${i+1}`;const state=document.createElement('span');state.className='state';state.textContent=c.state;title.append(name,state);
  const actions=document.createElement('div');for(const [label,fn] of [['预览结果',()=>{hasOutput=true;c.actual='42\n';c.state='演示结果';$('log').textContent='演示数据已显示。原型没有执行 C++ 源码。';render(true);}],['删除',()=>{cases.splice(i,1);render(true);}]]){const b=document.createElement('button');b.textContent=label;b.onclick=fn;actions.append(b);}head.append(title,actions);row.append(head);
  const fields=document.createElement('div');fields.className='fields';for(const [label,key,readonly] of [['输入','input',false],['实际输出 · 只读','actual',true],['预期输出','expected',false]]){const l=document.createElement('label');l.textContent=label;const t=document.createElement('textarea');t.setAttribute('aria-label',`用例 ${i+1} ${label}`);t.value=c[key];t.readOnly=readonly;t.spellcheck=false;t.oninput=()=>{c[key]=t.value;c.state='数据已修改';state.textContent=c.state;};l.append(t);fields.append(l);}row.append(fields);
  const compare=document.createElement('label');compare.className='compare';const cb=document.createElement('input');cb.type='checkbox';cb.checked=c.compare;cb.onchange=()=>{c.compare=cb.checked;};compare.append(cb,'比较预期输出');row.append(compare);$('visible').append(row);created++;
 }
 $('metrics').textContent=`当前 ${last-first} 行 / ${cases.length} 例`;
}
let pending=false;$('cases').onscroll=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;render();});};
$('add').onclick=()=>{cases.push({input:'',expected:'',actual:'',compare:true,state:'未运行'});$('cases').scrollTop=cases.length*ROW;render(true);};
$('add-first').onclick=()=>$('add').onclick();
$('stress').onclick=()=>{cases=Array.from({length:500},(_,i)=>({input:`${i}\n`,expected:`${i*2}\n`,actual:'',compare:true,state:'未运行'}));$('cases').scrollTop=0;render(true);};$('clear').onclick=()=>{hasOutput=false;$('log').textContent='';render();};
function splitter(id,axis){const el=$(id);let start;
 el.onpointerdown=e=>{start={x:e.clientX,y:e.clientY,w:document.querySelector('aside').getBoundingClientRect().width,h:$('messages').getBoundingClientRect().height};el.setPointerCapture(e.pointerId);};
 const resize=(w,h)=>{if(axis==='y')messagesResized=true;document.documentElement.style.setProperty('--right',`${Math.max(310,Math.min(innerWidth-260,w))}px`);document.documentElement.style.setProperty('--messages',`${Math.max(66,Math.min(innerHeight-300,h))}px`);render();};
 el.onpointermove=e=>{if(!start)return;resize(axis==='x'?start.w+start.x-e.clientX:start.w,axis==='y'?start.h+e.clientY-start.y:start.h);};el.onpointerup=()=>{start=null;};el.onpointercancel=()=>{start=null;};
 el.onkeydown=e=>{if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key))return;e.preventDefault();const w=document.querySelector('aside').getBoundingClientRect().width,h=$('messages').getBoundingClientRect().height;resize(w+(axis==='x'?(e.key==='ArrowLeft'?16:e.key==='ArrowRight'?-16:0):0),h+(axis==='y'?(e.key==='ArrowUp'?-16:e.key==='ArrowDown'?16:0):0));};
}splitter('width-handle','x');splitter('height-handle','y');new ResizeObserver(()=>render()).observe($('cases'));render();

document.querySelectorAll("[data-command]").forEach(button=>button.onclick=()=>{hasOutput=true;$("log").textContent="这是布局原型。请在原生 altaed 中编译、运行测试或交互执行。";render(true);});
