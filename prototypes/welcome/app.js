const params=new URLSearchParams(location.search);
document.documentElement.classList.toggle('reference',params.has('reference'));
const layout=await fetch('layout.json').then(r=>r.json());
for(const [key,value] of Object.entries(layout))if(typeof value==='number')document.documentElement.style.setProperty(`--${key}`,`${value}px`);
for(const [key,value] of Object.entries(layout.wordmark))document.documentElement.style.setProperty(`--wordmark-${key}`,value+'px');
document.documentElement.style.setProperty('--breath-duration',layout.breath_duration_seconds+'s');
document.documentElement.style.setProperty('--banner-min',layout.banner_opacity_min);
document.documentElement.style.setProperty('--banner-max',layout.banner_opacity_max);
const $=s=>document.querySelector(s);
const commands=await fetch('commands.json').then(r=>r.json());
const behaviors={'run.compile':'生成程序','run.run':'启动上次编译的程序','run.compile_and_run':'先编译，再启动'};
const shortcuts=new Map();
for(const id of Object.keys(behaviors)){
 const command=commands.find(c=>c.id===id);const row=document.createElement('tr');const label=document.createElement('td');label.textContent=command.label;const value=document.createElement('td');const input=document.createElement('input');input.className='control';input.value=command.macos;input.placeholder='未分配';input.setAttribute('aria-label',command.label);input.spellcheck=false;input.oninput=()=>{$('#shortcut-error').textContent='';input.setAttribute('aria-invalid','false');};value.append(input);shortcuts.set(id,input);const behavior=document.createElement('td');behavior.textContent=behaviors[id];row.append(label,value,behavior);$('tbody').append(row);
}
let timer;let page=Number(params.get('page')||1);let verification='pending';
function render(){
 document.querySelector(".welcome").classList.toggle("hero-page",page===1);
 for(const [index,id] of ['welcome-page','environment-page','shortcuts-page'].entries())document.getElementById(id).hidden=index+1!==page;
 $('#back').textContent=page===1?'退出':'上一步';$('#continue').textContent=page===1?'开始配置':page===2?'继续':'完成并进入 IDE';
 $('#continue').disabled=page===2&&!['ready','save-error'].includes(verification);
 $('#skip').hidden=page!==2||['ready','save-error'].includes(verification);
 $('#footer-status').textContent=page===2?(verification==='ready'?'环境已通过':verification==='checking'?'正在检测…':'可稍后在软件设置中配置'):'';
}
function state(value){
 clearTimeout(timer);verification=value;$('#preview-state').value=value;$('#environment-status').dataset.state=value;
 const messages={pending:'检测将编译并运行一段 C++20 程序。',checking:'正在编译并运行检测程序…',ready:'环境已就绪 · C++20 编译和执行检测通过。',error:'无法启动编译器。请检查路径，或选择已安装的 C++ 编译器后重试。','save-error':'环境已通过。'};
 $('#environment-message').textContent=messages[value];$('#success-icon').hidden=!['ready','save-error'].includes(value);for(const id of ['verify','compiler','choose','detect'])$('#'+id).disabled=value==='checking';render();
}
$('#preview-state').onchange=e=>state(e.target.value);
$('#compiler').oninput=()=>state('pending');
$('#verify').onclick=()=>{state('checking');timer=setTimeout(()=>state($('#compiler').value.includes('missing')||!$('#compiler').value.trim()?'error':'ready'),800);};
$('#detect').onclick=()=>{$('#compiler').value='/usr/bin/clang++';state('pending');};
$('#choose').onclick=()=>{$('#compiler').focus();$('#environment-message').textContent='原生应用会打开文件选择器；原型可直接填写路径。';};
$('#skip').onclick=()=>{clearTimeout(timer);state('pending');page=3;render();};
$('#back').onclick=()=>{if(page>1){page--;render();}else $('#footer-status').textContent='原生应用将退出。';};
$('#continue').onclick=()=>{
 if(page<3){if(page===2&&!['ready','save-error'].includes(verification))return;page++;render();return;}
 for(const [id,input] of shortcuts){const value=input.value.trim();for(const chord of value?value.split(/\s+/):[]){const parts=chord.split('-');const key=parts.pop();if(!key||parts.some(part=>!['cmd','ctrl','alt','shift','fn'].includes(part))){input.setAttribute('aria-invalid','true');$('#shortcut-error').textContent='快捷键格式无效，例如 cmd-shift-b；留空可停用。';input.focus();return;}}}
 if(verification==='save-error'){$('#shortcut-error').textContent='配置未保存，请检查目录写入权限后重试。';return;}
 $('#footer-status').textContent='原型演示完成，不写入本机配置。';
};
$('#config').onclick=()=>{$('#footer-status').textContent='原生应用会在文件管理器中显示 config.json。';};
state(params.get('state')||'pending');
if(verification==='save-error'&&page===3)$('#shortcut-error').textContent='配置未保存，请检查目录写入权限后重试。';
