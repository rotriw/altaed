'use strict';
// This page previews native layout and local interactions. It never compiles
// source, executes imported programs, or writes the user's files.
const $ = id => document.getElementById(id);
const ROW_HEIGHT = 196;
const sampleCases = () => [
  { input:'21\n', expected:'42 \t\n', actual:'42\n',  state:'Accepted' },
  { input:'21\n', expected:'43\n', actual:'42\n',  state:'WrongAnswer' }
];
const documents = [
  { name:'solution.cpp', mode:'trailing', absolute:'0.000001', relative:'0.000001', timeout:'2000', arguments:'', file:null, cases:sampleCases() },
  { name:'another.cpp', mode:'tokens', absolute:'0.000001', relative:'0.000001', timeout:'2000', arguments:'', file:null, cases:[{input:'4\n',expected:'8\n',actual:'',state:'未运行'}] }
];
let activeDocument = 0;
let visibleRange = '';
let settingsExpanded = false;
let messagesResized = false;
let importingDocument = null;
const currentDocument = () => documents[activeDocument];
if (new URLSearchParams(location.search).has('empty')) currentDocument().cases = [];
function icon(name) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg','svg');
  svg.setAttribute('aria-hidden','true');
  const use = document.createElementNS(svg.namespaceURI,'use');
  use.setAttribute('href',`#i-${name}`);
  svg.append(use);
  return svg;
}
function button(label, action, title) {
  const element = document.createElement('button');
  element.type = 'button'; element.textContent = label; element.onclick = action;
  if (title) element.title = title;
  return element;
}
function updateVerdict(container, testCase, index) {
  container.replaceChildren(); container.className = 'case-state';
  if (testCase.state === 'Accepted') {
    container.classList.add('accepted'); container.append(icon('check'), 'Accepted');
  } else if (testCase.state === 'WrongAnswer') {
    container.classList.add('wrong-answer');
    container.append(icon('close'), button('WrongAnswer', () => showDiff(index), '打开输出差异'));
  } else container.textContent = testCase.state;
}
function renderCases(force = false) {
  const cases = currentDocument().cases;
  const first = Math.max(0, Math.floor($('cases').scrollTop / ROW_HEIGHT)-1);
  const last = Math.min(cases.length, Math.ceil(($('cases').scrollTop + $('cases').clientHeight)/ROW_HEIGHT)+1);
  const range = `${activeDocument}:${first}:${last}`;
  $('count').textContent = cases.length;
  $('empty-tests').hidden = cases.length !== 0;
  $('spacer').style.height = `${cases.length * ROW_HEIGHT}px`;
  if (!force && visibleRange === range) return;
  visibleRange = range; $('visible').replaceChildren();
  $('visible').style.transform = `translateY(${first * ROW_HEIGHT}px)`;
  for (let index = first; index < last; index++) {
    const testCase = cases[index];
    const row = document.createElement('section'); row.className = 'case'; row.setAttribute('aria-label',`用例 ${index+1}`);
    const head = document.createElement('div'); head.className = 'case-head';
    const title = document.createElement('div'); title.className = 'case-title';
    const name = document.createElement('strong'); name.textContent = `用例 ${index+1}`;
    const state = document.createElement('span'); updateVerdict(state,testCase,index); title.append(name,state);
    const actions = document.createElement('div'); actions.className = 'case-actions';
    const diffButton = button('Diff',()=>showDiff(index)); diffButton.disabled = !testCase.actual || testCase.expected.length===0;
    actions.append(diffButton, button('运行',()=>previewResults(index)), button('删除',()=>{ cases.splice(index,1); renderCases(true); }));
    head.append(title,actions); row.append(head);
    const fields = document.createElement('div'); fields.className = 'fields';
    for (const [label,key,readonly] of [['输入','input',false],['实际输出','actual',true],['预期输出','expected',false]]) {
      const field = document.createElement('label'); field.className = 'field';
      const heading = document.createElement('span'); heading.className = 'field-heading'; heading.textContent = label;
      const text = document.createElement('textarea'); text.className = 'test-text'; text.setAttribute('aria-label',`用例 ${index+1} ${label}`);
      text.value = testCase[key]; text.readOnly = readonly; text.spellcheck = false;
      if(key==='expected')text.placeholder='留空仅观察输出';
      text.oninput = () => { testCase[key] = text.value; testCase.state = '已修改'; updateVerdict(state,testCase,index); };
      field.append(heading,text); fields.append(field);
    }
    row.append(fields);
    $('visible').append(row);
  }
  $('metrics').textContent=`${Math.max(0,last-first)} 行 / ${cases.length} 例`;
}
function markStale() {
  for (const testCase of currentDocument().cases) if (testCase.actual) testCase.state = '设置已修改';
  $('status').textContent=`${currentDocument().name} 的比较设置已修改；其他文档保持独立。`;
  renderCases(true);
}
function renderChecker() {
  const doc = currentDocument();
  const previous = $('checker').querySelector('[data-custom]'); if (previous) previous.remove();
  if (doc.file) {
    const option = document.createElement('option'); option.value='executable'; option.textContent=doc.file; option.dataset.custom='true'; $('checker').append(option);
  }
  $('checker').value=doc.mode; $('checker').title=doc.file && doc.mode==='executable' ? doc.file : $('checker').selectedOptions[0].textContent;
  $('checker-settings').setAttribute('aria-expanded',settingsExpanded);
  $('checker-options').hidden=!settingsExpanded;
  $('float-options').hidden=doc.mode!=='float'; $('executable-options').hidden=doc.mode!=='executable';
  $('text-options').hidden=doc.mode==='float'||doc.mode==='executable';
  $('text-options').textContent={trailing:'忽略行末空格、Tab 和文末换行，保留行中空格与中间空行。', exact:'逐字节比较，行末空格和末尾换行均参与比较。', tokens:'逐个比较由空白分隔的字符串，区分大小写。', ncmp:'按有符号 64 位整数比较，不经过浮点转换。', nyesno:'只接受 YES 和 NO，比较时不区分大小写。', rcmp4:'比较有限浮点数，最大绝对误差或相对误差为 1e-4。', rcmp6:'比较有限浮点数，最大绝对误差或相对误差为 1e-6。', rcmp9:'比较有限浮点数，最大绝对误差或相对误差为 1e-9。'}[doc.mode] || '';
  for (const [id,key] of [['absolute','absolute'],['relative','relative'],['checker-timeout','timeout'],['checker-arguments','arguments']]) $(id).value=doc[key];
  $('checker-path').textContent=doc.file||''; validateParameters();
}
function validateParameters() {
  const doc=currentDocument(); let error='';
  if(doc.mode==='float' && [doc.absolute,doc.relative].some(value=>value.trim()==='' || !Number.isFinite(Number(value)) || Number(value)<0)) error='容差必须是非负有限数。';
  if(doc.mode==='executable' && (!/^\d+$/.test(doc.timeout) || Number(doc.timeout)<1)) error='比较器超时必须是大于零的整数。';
  $('checker-error').textContent=error; $('checker-error').hidden=!error; return !error;
}
function switchDocument(index) {
  activeDocument=index; $('cases').scrollTop=0;
  document.querySelectorAll('[data-document]').forEach(tab=>{ const active=Number(tab.dataset.document)===index; tab.setAttribute('aria-selected',active); tab.tabIndex=active?0:-1; });
  $('document-scope').textContent='行 1，列 1';
  document.title=currentDocument().name+' — altaed 原型';
  $('log').textContent='布局原型。当前文档的测试数据与比较设置独立。';
  renderChecker(); renderCases(true);
}
function previewResults(onlyIndex) {
  if(!validateParameters()) { settingsExpanded=true; renderChecker(); return; }
  const doc=currentDocument();
  for(const [index,testCase] of doc.cases.entries()) {
    if(onlyIndex!==undefined && index!==onlyIndex) continue;
    // Deliberate fixture for the displayed *2 example, not a C++ runtime.
    testCase.actual=`${Number(testCase.input.trim())*2}\n`;
    const trimLines=text=>text.replace(/[ \t\r\n]+$/g,'').split('\n').map(line=>line.replace(/[ \t\r]+$/g,'')).join('\n');
    const tokens=text=>text.trim().split(/\s+/);
    let accepted=false;
    if(doc.mode==='trailing') accepted=trimLines(testCase.expected)===trimLines(testCase.actual);
    else if(doc.mode==='exact') accepted=testCase.expected===testCase.actual;
    else if(doc.mode==='tokens') accepted=JSON.stringify(tokens(testCase.expected))===JSON.stringify(tokens(testCase.actual));
    else if(doc.mode==='ncmp') {
      const a=tokens(testCase.expected), b=tokens(testCase.actual), valid=value=>/^[+-]?\d+$/.test(value)&&BigInt(value)>=-(1n<<63n)&&BigInt(value)<(1n<<63n);
      accepted=a.length===b.length&&a.every((value,index)=>valid(value)&&valid(b[index])&&BigInt(value)===BigInt(b[index]));
    } else if(doc.mode==='nyesno') {
      const a=tokens(testCase.expected), b=tokens(testCase.actual);
      accepted=a.length===b.length&&a.every((value,index)=>/^(yes|no)$/i.test(value)&&value.toUpperCase()===b[index].toUpperCase());
    } else if(doc.mode.startsWith('rcmp')) {
      const epsilon={rcmp4:1e-4,rcmp6:1e-6,rcmp9:1e-9}[doc.mode]+1e-15;
      const a=tokens(testCase.expected),b=tokens(testCase.actual);
      accepted=a.length===b.length&&a.every((value,index)=>{
        const x=Number(value),y=Number(b[index]),lo=x*(1-epsilon),hi=x*(1+epsilon);
        return Number.isFinite(x)&&Number.isFinite(y)&&(Math.abs(x-y)<=epsilon||(y>=Math.min(lo,hi)&&y<=Math.max(lo,hi)));
      });
    } else if(doc.mode==='float') {
      const a=tokens(testCase.expected), b=tokens(testCase.actual);
      accepted=a.length===b.length && a.every((value,index)=>{
        const x=Number(value), y=Number(b[index]);
        return Number.isFinite(x)&&Number.isFinite(y) ? Math.abs(x-y)<=Number(doc.absolute)||Math.abs(x-y)/Math.max(Math.abs(x),Math.abs(y))<=Number(doc.relative) : value===b[index];
      });
    }
    testCase.state=testCase.expected.length===0?'运行结束':doc.mode==='executable'?'比较器未执行':accepted?'Accepted':'WrongAnswer';
  }
  $('log').textContent=doc.mode==='executable'?'原型仅预览文件选择，不执行导入的比较器。':'演示结果已更新。原型没有编译或运行 C++ 源码。';
  $('status').textContent='演示结果 · 请在原生应用中进行真实编译测试。'; renderCases(true);
}
function showDiff(index) {
  const testCase=currentDocument().cases[index];
  $('diff-title').textContent=`用例 ${index+1} · 输出差异（演示）`;
  const visible=text=>text.replace(/ /g,'·').replace(/\t/g,'⇥').replace(/\r/g,'␍').replace(/\n/g,'↵\n');
  $('diff-expected').textContent=visible(testCase.expected); $('diff-actual').textContent=visible(testCase.actual);
  $('diff-dialog').showModal();
}
let scrolling=false;
for(const [source,target] of [['diff-expected','diff-actual'],['diff-actual','diff-expected']]) $(source).onscroll=()=>{
  if(scrolling)return; scrolling=true; $(target).scrollTop=$(source).scrollTop; $(target).scrollLeft=$(source).scrollLeft; requestAnimationFrame(()=>{scrolling=false;});
};
$('diff-close').onclick=()=>$('diff-dialog').close();
$('checker').onchange=()=>{currentDocument().mode=$('checker').value;renderChecker();markStale();};
$('checker-settings').onclick=()=>{settingsExpanded=!settingsExpanded;renderChecker();};
$('import-checker').onclick=()=>{importingDocument=activeDocument;$('checker-file').click();};
$('checker-file').onchange=()=>{
  const file=$('checker-file').files[0]; if(!file)return;
  const target=documents[importingDocument];
  if(!target)return;
  if(/\.(cpp|cc|cxx|c|py|js|sh|bat|cmd)$/i.test(file.name)) { $('status').textContent='请选择现成的可执行比较器，不支持源码或脚本。'; $('checker-file').value=''; return; }
  target.file=file.name; target.mode='executable';
  for(const testCase of target.cases) if(testCase.actual)testCase.state='设置已修改';
  if(importingDocument===activeDocument) {renderChecker();renderCases(true);}
  $('status').textContent=`${target.name} 已选择 ${file.name}。原型只展示名称，不读取或运行文件。`; $('checker-file').value='';
};
for(const [id,key] of [['absolute','absolute'],['relative','relative'],['checker-timeout','timeout'],['checker-arguments','arguments']]) $(id).oninput=()=>{currentDocument()[key]=$(id).value;validateParameters();markStale();};
document.querySelectorAll('[data-document]').forEach(tab=>{
  tab.onclick=()=>switchDocument(Number(tab.dataset.document));
  tab.onkeydown=event=>{if(!['ArrowLeft','ArrowRight'].includes(event.key))return;event.preventDefault();switchDocument(1-activeDocument);document.querySelector('[aria-selected="true"]').focus();};
});
let pending=false;
$('cases').onscroll=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;renderCases();});};
$('add').onclick=()=>{currentDocument().cases.push({input:'',expected:'',actual:'',state:'未运行'});$('spacer').style.height=`${currentDocument().cases.length*ROW_HEIGHT}px`;$('cases').scrollTop=currentDocument().cases.length*ROW_HEIGHT;renderCases(true);};
$('add-first').onclick=()=>$('add').click();
$('stress').onclick=()=>{currentDocument().cases=Array.from({length:500},(_,i)=>({input:`${i}\n`,expected:`${i*2}\n`,actual:'',state:'未运行'}));$('cases').scrollTop=0;renderCases(true);};
$('empty').onclick=()=>{currentDocument().cases=[];$('cases').scrollTop=0;renderCases(true);};
$('reset').onclick=()=>{currentDocument().cases=sampleCases();$('cases').scrollTop=0;renderCases(true);};
$('clear').onclick=()=>{$('log').textContent='';if(!messagesResized)document.documentElement.style.setProperty('--messages','76px');};
document.querySelectorAll('[data-command]').forEach(control=>control.onclick=()=>{
  if(control.dataset.command==='tests') previewResults(); else $('status').textContent='布局原型 · 编译和执行请在原生 altaed 中进行。';
});
function splitter(id,axis) {
  const element=$(id); let start;
  element.onpointerdown=event=>{start={x:event.clientX,y:event.clientY,w:document.querySelector('aside').getBoundingClientRect().width,h:$('messages').getBoundingClientRect().height};element.setPointerCapture(event.pointerId);};
  const resize=(width,height)=>{
    const maxWidth=Math.max(340,innerWidth-281), maxHeight=Math.max(66,innerHeight-330);
    const w=Math.max(340,Math.min(maxWidth,width)), h=Math.max(66,Math.min(maxHeight,height));
    if(axis==='y')messagesResized=true;
    document.documentElement.style.setProperty('--right',`${w}px`);document.documentElement.style.setProperty('--messages',`${h}px`);
    $('width-handle').setAttribute('aria-valuenow',Math.round(w));$('width-handle').setAttribute('aria-valuemax',maxWidth);
    $('height-handle').setAttribute('aria-valuenow',Math.round(h));$('height-handle').setAttribute('aria-valuemax',maxHeight);renderCases();
  };
  element.onpointermove=event=>{if(!start)return;resize(axis==='x'?start.w+start.x-event.clientX:start.w,axis==='y'?start.h+event.clientY-start.y:start.h);};
  element.onpointerup=element.onpointercancel=()=>{start=null;};
  element.onkeydown=event=>{if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key))return;event.preventDefault();resize(document.querySelector('aside').getBoundingClientRect().width+(axis==='x'?(event.key==='ArrowLeft'?16:event.key==='ArrowRight'?-16:0):0),$('messages').getBoundingClientRect().height+(axis==='y'?(event.key==='ArrowUp'?-16:event.key==='ArrowDown'?16:0):0));};
}
splitter('width-handle','x');splitter('height-handle','y');
new ResizeObserver(()=>renderCases()).observe($('cases'));
renderChecker();renderCases();

let statusPreview='idle';
let selectedColorTheme=null;
const customStatusColors={compile:false,run:false};
function previewStatus(phase) {
  statusPreview=phase;
  const bar=$('statusbar');
  bar.style.background=phase==='idle'?'var(--paper)':$(phase==='compile'?'compile-color':'run-color').value;
  if(phase==='idle')bar.style.color='var(--muted)';
  else {
    const rgb=parseInt($(phase==='compile'?'compile-color':'run-color').value.slice(1),16);
    const luminance=value=>[16,8,0].map(shift=>((value>>shift)&255)/255).map(v=>v<=0.04045?v/12.92:((v+0.055)/1.055)**2.4).reduce((sum,v,i)=>sum+v*[0.2126,0.7152,0.0722][i],0);
    const contrast=value=>(Math.max(luminance(rgb),luminance(value))+.05)/(Math.min(luminance(rgb),luminance(value))+.05);
    bar.style.color=contrast(0x27313b)>=4.5?'#27313b':contrast(0xffffff)>=contrast(0)?'#ffffff':'#000000';
  }
  $('status').textContent={idle:'就绪',compile:'正在编译…',run:'正在运行测试…'}[phase];
}
$('preview-compile').onclick=()=>previewStatus('compile');
$('preview-run').onclick=()=>previewStatus('run');
$('preview-idle').onclick=()=>previewStatus('idle');
$('status-settings').onclick=()=>$('status-colors').togglePopover();
for(const [id,phase] of [['compile-color','compile'],['run-color','run']])$(id).oninput=()=>{customStatusColors[phase]=true;previewStatus(statusPreview);};
$('toggle-output').onclick=()=>{$('status').textContent='布局原型 · 详细编译与运行输出在原生应用中查看。';};

function applyColorTheme(theme) {
  selectedColorTheme=theme;
  const style=theme.style;
  for(const [variable,key] of Object.entries({'paper':'editor.background','subtle':'surface.background','readonly':'editor.subheader.background','text':'text','muted':'text.muted','accent':'text.accent','line':'border','editor-text':'editor.foreground','success':'success','error':'error','diff-error':'error.background','diff-success':'success.background'}))document.documentElement.style.setProperty('--'+variable,style[key]);
  document.documentElement.style.colorScheme=theme.appearance;
  for(const element of document.querySelectorAll('[data-token]')) {
    const token=style.syntax[element.dataset.token];
    element.style.color=token.color;
    element.style.fontStyle=token.font_style||'normal';
    element.style.fontWeight=token.font_weight||400;
  }
  const mix=(a,b)=>'#'+[16,8,0].map(shift=>{const x=parseInt(a.slice(1,7),16),y=parseInt(b.slice(1,7),16);return Math.floor((((x>>shift)&255)*88+((y>>shift)&255)*12)/100).toString(16).padStart(2,'0');}).join('');
  for(const phase of ['compile','run'])if(!customStatusColors[phase])$(phase==='compile'?'compile-color':'run-color').value=mix(style['editor.background'],style[phase==='compile'?'text.accent':'success']);
  previewStatus(statusPreview);
}
fetch('themes.json').then(response=>{if(!response.ok)throw new Error('主题文件无法读取');return response.json();}).then(family=>{
  for(const theme of family.themes){const option=document.createElement('option');option.value=theme.id;option.textContent=theme.label;$('color-theme').append(option);}
  const initial=new URLSearchParams(location.search).get('theme')||'default-light';
  $('color-theme').value=initial;
  applyColorTheme(family.themes.find(theme=>theme.id===initial)||family.themes[0]);
  $('color-theme').onchange=()=>applyColorTheme(family.themes.find(theme=>theme.id===$('color-theme').value));
}).catch(error=>{$('status').textContent=error.message;});

const quickSettings=document.querySelector('#global-settings');
const quickMenu=document.querySelector('#quick-settings-menu');
quickSettings.onclick=()=>{quickMenu.hidden=!quickMenu.hidden;quickSettings.setAttribute('aria-expanded',String(!quickMenu.hidden));if(!quickMenu.hidden)quickMenu.querySelector('a').focus();};
document.querySelector('.quick-settings').addEventListener('focusout',event=>{if(!document.querySelector('.quick-settings').contains(event.relatedTarget)){quickMenu.hidden=true;quickSettings.setAttribute('aria-expanded','false');}});
quickMenu.onkeydown=event=>{const entries=[...quickMenu.querySelectorAll('a')];if(event.key==='Escape'){quickMenu.hidden=true;quickSettings.setAttribute('aria-expanded','false');quickSettings.focus();}else if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();const next=(entries.indexOf(document.activeElement)+(event.key==='ArrowDown'?1:-1)+entries.length)%entries.length;entries[next].focus();}};
document.querySelector('#title-open-file').onclick=()=>document.querySelector('#title-file').click();
document.querySelector('#title-file').onchange=event=>{const file=event.target.files[0];if(file)document.querySelector('#tab-first').textContent=file.name;};

const platformPreview=document.querySelector('#platform-preview');
platformPreview.value=new URLSearchParams(location.search).get('platform')==='windows'?'windows':'macos';
const updatePlatform=()=>document.documentElement.classList.toggle('is-windows',platformPreview.value==='windows');
platformPreview.onchange=updatePlatform;updatePlatform();
