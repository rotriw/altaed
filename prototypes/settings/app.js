const $ = selector => document.querySelector(selector);
document.documentElement.classList.toggle('reference',new URLSearchParams(location.search).has('reference'));
const documentSettings = new URLSearchParams(location.search).has('document');
const contextNote = documentSettings ? '修改只应用到当前文档' : '对整个软件生效';
$('h1').textContent = documentSettings ? '文档运行配置' : '设置';
$('#scope-note').textContent = contextNote;
$('#other-window').textContent = documentSettings ? '软件设置原型' : '文档运行配置原型';
$('#other-window').href = documentSettings ? './' : '?document';
const schema = await fetch(documentSettings ? 'document-schema.json' : 'schema.json').then(response => {
  if (!response.ok) throw new Error('无法载入设置描述');
  return response.json();
});
let commandDefinitions=[];
if(!documentSettings){
  commandDefinitions=await fetch('commands.json').then(response=>response.json());
  schema.categories.push({id:'core.keybindings',label:'快捷键',description:'点击“应用”后生效；留空可停用对应快捷键。'});
  for(const command of commandDefinitions)schema.settings.push({id:`core.keybindings.${command.id}`,category:'core.keybindings',label:command.label,description:'使用 cmd / ctrl / alt / shift 组合，多个按键序列以空格分隔；留空停用。',kind:'text',default:command.macos,placeholder:'未分配',control_width:320});
}
for (const [key, value] of Object.entries(schema.layout)) document.documentElement.style.setProperty(`--${key}`, `${value}px`);
if(new URLSearchParams(location.search).get('reference')==='narrow'){document.documentElement.style.setProperty('--width','390px');document.documentElement.style.setProperty('--height','760px');}
const base = Object.fromEntries(schema.settings.map(setting => [setting.id, setting.default]));
let saved = {...base};
let draft = {...base};
let category = schema.categories[0].id;
let query = '';
let errors = {};
const example = {
  owner: 'plugin.example',
  categories: [{id:'plugin.example.submit', label:'提交示例', description:'扩展结构示例，不连接服务器或提交代码。', source:'示例'}],
  settings: [
    {id:'plugin.example.channel',category:'plugin.example.submit',label:'提交通道',description:'插件可注册选择项，并使用自己的稳定值。',kind:'choice',default:'direct',options:[{value:'direct',label:'本地直连'},{value:'relay',label:'加密中继'}]},
    {id:'plugin.example.confirm',category:'plugin.example.submit',label:'提交前确认',description:'插件可添加开关，沿用相同的表单与保存方式。',kind:'toggle',default:true}
  ]
};
function activeSchema() {
  return $('#extension-preview').checked ? {categories:[...schema.categories,...example.categories],settings:[...schema.settings,...example.settings]} : schema;
}
function visible(setting) { return !setting.visible_when || draft[setting.visible_when.id] === setting.visible_when.value; }
function matching(setting) {
  const group = activeSchema().categories.find(item => item.id === setting.category);
  return `${setting.label} ${setting.description} ${setting.id} ${group?.label} ${(setting.options||[]).map(option=>option.label).join(' ')}`.toLowerCase().includes(query);
}
function fields() { return activeSchema().settings.filter(setting => visible(setting) && (query ? matching(setting) : setting.category === category)); }
function validate(setting) {
  const value = draft[setting.id];
  if(setting.category==='core.keybindings'&&String(value).trim()){
    const modifiers=new Set(['cmd','ctrl','alt','shift','fn']);
    for(const chord of value.trim().split(/\s+/)){const parts=chord.split('-');const key=parts.pop();if(!key||parts.some(part=>!modifiers.has(part))||new Set(parts).size!==parts.length)return '快捷键格式无效，例如 cmd-shift-b；留空可停用。';}
  }
  if (setting.required && !value.trim()) return `${setting.label}不能为空。`;
  if (setting.max !== undefined && Number(value) > setting.max) return `${setting.label}不能大于 ${setting.max}。`;
  if (setting.kind === 'integer' && (!/^\d+$/.test(value.trim()) || !Number.isSafeInteger(Number(value)) || Number(value) < setting.min)) return `请输入不小于 ${setting.min} 的整数。`;
  if (setting.kind === 'decimal' && (value.trim()==='' || !Number.isFinite(Number(value)) || Number(value)<setting.min)) return `请输入不小于 ${setting.min} 的有限数值。`;
  return '';
}
function updateStatus(message) {
  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  $('#apply').disabled = !dirty;
  $('h1').classList.toggle('dirty',dirty);
  $('#status').textContent = message || (documentSettings?'随文档保存运行配置':'更改后点击应用保存');
}
function nav() {
  const navigation = $('nav'); navigation.replaceChildren();
  let extensions = false;
  for (const item of activeSchema().categories) {
    if (item.source && !extensions) { const heading=document.createElement('p'); heading.className='extension-heading'; heading.textContent='插件'; navigation.append(heading); extensions=true; }
    const button=document.createElement('button'); button.textContent=item.label; button.setAttribute('aria-current',String(category===item.id && !query));
    if (item.source) { const source=document.createElement('span'); source.className='source'; source.textContent=item.source; button.append(source); }
    button.onclick=()=>{category=item.id;query='';$('#search').value='';render();}; navigation.append(button);
  }
}
function render() {
  nav();
  const selected=activeSchema().categories.find(item=>item.id===category) || schema.categories[0];
  $('#page-title').textContent=query?'搜索结果':selected.label;
  $('#page-description').textContent=query?`包含“${$('#search').value.trim()}”的设置`:selected.description;
  $('#page-description').hidden=!query&&category!=='core.keybindings';
  $('#empty p').textContent=documentSettings?'试试“编译器”“超时”或“比较”。':'试试“字体”“换行”或“工作台”。';
  const container=$('#fields');container.replaceChildren();
  let previousCategory='';
  const shown=fields();
  for (const setting of shown.filter(item=>item.category!=='core.keybindings')) {
    if (query && previousCategory!==setting.category) {
      const heading=document.createElement('div');heading.className='group-heading';heading.textContent=activeSchema().categories.find(item=>item.id===setting.category).label;container.append(heading);previousCategory=setting.category;
    }
    const row=document.createElement('div');row.className='setting';row.dataset.id=setting.id;
    const labelRow=document.createElement('div');labelRow.className='setting-label';
    const label=document.createElement('label');label.className='title';label.htmlFor=setting.id;label.textContent=setting.label;labelRow.append(label);
    if (setting.unit){const unit=document.createElement('span');unit.className='unit';unit.textContent=setting.unit;labelRow.append(unit);}row.append(labelRow);
    if(setting.kind==='choice'){
      const choices=document.createElement('div');choices.className='choices';choices.id=setting.id;choices.setAttribute('role','radiogroup');choices.setAttribute('aria-label',setting.label);
      setting.options.forEach(option=>{const button=document.createElement('button');button.type='button';button.textContent=option.label;button.setAttribute('role','radio');button.setAttribute('aria-checked',String(draft[setting.id]===option.value));button.onclick=()=>{draft[setting.id]=option.value;render();$('#'+CSS.escape(setting.id)).querySelector('[aria-checked=true]').focus();};choices.append(button);});row.append(choices);
    }else if(setting.kind==='toggle'){
      const toggle=document.createElement('label');toggle.className='toggle';const input=document.createElement('input');input.type='checkbox';input.id=setting.id;input.checked=draft[setting.id];input.onchange=()=>{draft[setting.id]=input.checked;updateStatus();};toggle.append(input,document.createTextNode('启用'));row.append(toggle);
    }else{
      const input=document.createElement(setting.kind==='multiline'?'textarea':'input');input.id=setting.id;input.className='control';if(setting.control_width)input.style.maxWidth=setting.control_width+'px';input.value=draft[setting.id];input.placeholder=setting.placeholder||'';input.spellcheck=false;input.setAttribute('aria-describedby',`${setting.id}-help ${setting.id}-error`);input.setAttribute('aria-invalid',String(Boolean(errors[setting.id])));
      if(setting.kind==='integer'||setting.kind==='decimal')input.inputMode=setting.kind==='integer'?'numeric':'decimal';
      input.oninput=()=>{draft[setting.id]=input.value;delete errors[setting.id];input.setAttribute('aria-invalid','false');row.querySelector('.error').textContent='';updateStatus();};
      input.onblur=()=>{errors[setting.id]=validate(setting);row.querySelector('.error').textContent=errors[setting.id];input.setAttribute('aria-invalid',String(Boolean(errors[setting.id])));};row.append(input);
    }
    const help=document.createElement('p');help.className='help';help.id=`${setting.id}-help`;help.textContent=setting.description;
    const error=document.createElement('p');error.className='error';error.id=`${setting.id}-error`;error.textContent=errors[setting.id]||'';error.setAttribute('role','alert');row.insertBefore(help,row.children[1]);row.append(error);container.append(row);
  }
  const bindings=shown.filter(item=>item.category==='core.keybindings');
  if(bindings.length){
    const hint=document.createElement('p');hint.className='keyboard-help';hint.textContent='使用 cmd / ctrl / alt / shift 组合；多个按键序列以空格分隔，留空停用。';container.append(hint);
    const table=document.createElement('table');table.className='keybindings-table';
    const head=document.createElement('thead');const heading=document.createElement('tr');
    for(const text of ['命令','快捷键','操作']){const cell=document.createElement('th');cell.textContent=text;heading.append(cell);}head.append(heading);table.append(head);
    const body=document.createElement('tbody');
    for(const setting of bindings){
      const row=document.createElement('tr');const name=document.createElement('td');const label=document.createElement('label');label.htmlFor=setting.id;label.textContent=setting.label;name.append(label);row.append(name);
      const value=document.createElement('td');const input=document.createElement('input');input.className='control';input.id=setting.id;input.value=draft[setting.id];input.placeholder='未分配';input.spellcheck=false;input.setAttribute('aria-invalid',String(Boolean(errors[setting.id])));input.setAttribute('aria-describedby',setting.id+'-error');const error=document.createElement('p');error.className='error';error.id=setting.id+'-error';error.setAttribute('role','alert');error.textContent=errors[setting.id]||'';input.oninput=()=>{draft[setting.id]=input.value;delete errors[setting.id];error.textContent='';input.setAttribute('aria-invalid','false');updateStatus();};input.onblur=()=>{errors[setting.id]=validate(setting);error.textContent=errors[setting.id];input.setAttribute('aria-invalid',String(Boolean(errors[setting.id])));};value.append(input,error);row.append(value);
      const action=document.createElement('td');const reset=document.createElement('button');reset.type='button';reset.textContent='恢复';reset.setAttribute('aria-label',`恢复${setting.label}的默认快捷键`);reset.onclick=()=>{draft[setting.id]=setting.default;input.value=setting.default;delete errors[setting.id];error.textContent='';input.setAttribute('aria-invalid','false');updateStatus();input.focus();};action.append(reset);row.append(action);body.append(row);
    }table.append(body);container.append(table);
  }
  $('#empty').hidden=shown.length!==0;
  updateStatus();
}
$('#fields').onsubmit=event=>event.preventDefault();
$('#search').oninput=event=>{query=event.target.value.trim().toLowerCase();render();};
$('#clear-search').onclick=()=>{query='';$('#search').value='';render();$('#search').focus();};
$('#reset').onclick=()=>{for(const setting of fields())draft[setting.id]=setting.default;errors={};render();updateStatus('默认值已填入，点击“应用”保存');};
$('#cancel').onclick=()=>{draft={...saved};errors={};render();updateStatus('已取消未应用的修改');};
$('#apply').onclick=()=>{
  errors={};for(const setting of activeSchema().settings.filter(visible)){const error=validate(setting);if(error)errors[setting.id]=error;}
  if(Object.keys(errors).length){const first=activeSchema().settings.find(setting=>errors[setting.id]);category=first.category;query='';$('#search').value='';render();document.getElementById(first.id).focus();updateStatus('请修正标记的设置');return;}
  saved={...draft};updateStatus(documentSettings?'已应用到当前文档':'软件设置已应用');
};
$('#extension-preview').onchange=()=>{
  for(const setting of example.settings)if(!(setting.id in draft)){draft[setting.id]=setting.default;saved[setting.id]=setting.default;}
  if(!$('#extension-preview').checked && category.startsWith('plugin.'))category=schema.categories[0].id;render();
};
document.addEventListener('keydown',event=>{if((event.metaKey||event.ctrlKey)&&event.key==='f'){event.preventDefault();$('#search').focus();}if(event.key==='Escape'&&query){query='';$('#search').value='';render();}});
render();
