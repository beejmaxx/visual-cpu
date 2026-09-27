import {$} from './inspectors.js';
import {createWorkspace} from './workspace.js';

export function createViewport() {
  const svg=$('diagram'), area=$('machine-viewport');
  const workspace=document.createElement('div');workspace.className='machine-workspace';
  const core=document.querySelector('.core'), inspector=document.querySelector('.inspection');
  core.replaceWith(workspace);workspace.append(document.querySelector('.clock-focus'),core,inspector);
  const layout=createWorkspace();
  const close=document.createElement('button');close.className='close-inspection';close.textContent='×';close.setAttribute('aria-label','Close component inspector');
  inspector.querySelector('.tabs').append(close);close.onclick=()=>{workspace.classList.remove('inspecting','wide-inspector');expand.textContent='Expand';$('viewport-context').textContent='Drag background to pan · wheel to zoom';focus('machine');};
  const expand=document.createElement('button');expand.textContent='Expand';expand.className='expand-inspection';expand.title='Show the inspector at full width';
  close.before(expand);expand.onclick=()=>{const wide=workspace.classList.toggle('wide-inspector');expand.textContent=wide?'Dock':'Expand';};
  const presets={machine:[-10,-10,1405,555],core:[10,0,980,520],memory:[975,-10,435,550],alu:[325,140,655,365],muldiv:[325,160,655,365],decode:[170,10,610,115],ram:[1090,355,315,165],cache:[990,0,400,515]};
  let box=[...presets.machine], drag=null;
  function draw(){svg.setAttribute('viewBox',box.join(' '));$('zoom-label').textContent=`${Math.round(1405/box[2]*100)}%`;}
  function focus(name){box=[...(presets[name]||presets.machine)];draw();document.querySelectorAll('[data-focus]').forEach(el=>el.classList.toggle('active',el.dataset.focus===name));}
  function point(e){
    const r=svg.getBoundingClientRect();
    const scale=Math.min(r.width/box[2],r.height/box[3])||1;
    return [box[0]+(e.clientX-r.left-(r.width-box[2]*scale)/2)/scale,box[1]+(e.clientY-r.top-(r.height-box[3]*scale)/2)/scale];
  }
  function zoom(factor,anchor=[box[0]+box[2]/2,box[1]+box[3]/2]){
    const width=Math.max(160,Math.min(2200,box[2]*factor)),ratio=width/box[2];
    box=[anchor[0]-(anchor[0]-box[0])*ratio,anchor[1]-(anchor[1]-box[1])*ratio,width,box[3]*ratio];draw();
    document.querySelectorAll('[data-focus]').forEach(el=>el.classList.remove('active'));
  }
  document.querySelectorAll('[data-focus]').forEach(el=>el.onclick=()=>focus(el.dataset.focus));
  $('zoom-in').onclick=()=>zoom(.8);$('zoom-out').onclick=()=>zoom(1.25);
  area.addEventListener('wheel',e=>{e.preventDefault();zoom(Math.exp(Math.max(-.3,Math.min(.3,e.deltaY*.002))),point(e));},{passive:false});
  area.onpointerdown=e=>{
    if(e.button!==0||e.target.closest('[role=button],button,[data-unit]'))return;
    drag={at:point(e),box:[...box]};area.setPointerCapture?.(e.pointerId);e.preventDefault();
  };
  area.onpointermove=e=>{if(!drag)return;const at=point(e);box[0]+=drag.at[0]-at[0];box[1]+=drag.at[1]-at[1];draw();};
  area.onpointerup=area.onpointercancel=()=>{drag=null;};
  area.onkeydown=e=>{if(e.target!==area)return;if(['+','=','-','0','ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)){e.preventDefault();e.stopPropagation();if(e.key==='0')focus('machine');else if(e.key==='+'||e.key==='=')zoom(.8);else if(e.key==='-')zoom(1.25);else{box[e.key==='ArrowLeft'||e.key==='ArrowRight'?0:1]+=(e.key==='ArrowLeft'||e.key==='ArrowUp'?-1:1)*box[2]*.1;draw();}}};
  return {
    focus,showTab:layout.showTab,
    inspect(name){workspace.classList.add('inspecting');$('viewport-context').textContent=`${name==='cache'?'Cache lookup':name==='alu'?'ALU':name==='muldiv'?'Multiply / divide':name==='decode'?'Decoder':'RAM'} · inspector open`;},
    reset(){workspace.classList.remove('inspecting','wide-inspector');expand.textContent='Expand';focus('machine');$('viewport-context').textContent='Drag background to pan · wheel to zoom';},
  };
}
