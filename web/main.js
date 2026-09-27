import {$,hex,escape,createInspectors} from './inspectors.js';
import {createExperience} from './experience.js';

const descriptions={flow:'samples[i] × factor → products[i] · UART output: 126',alu:'Integer operations · load / store · cold and warm reads',array_sum:'Array sum · 16 unsigned integers · 2 passes',fibonacci:'Recursive Fibonacci · n = 0–9',sort:'Quicksort · 12 integers',echo:'UART echo · exits after one line'};
const speeds=[.5,4,40,4000,40000];
let Simulator,sim,state,rows=[],breakpoints=new Set(),running=false;
let temporaryBreakpoint=null,skipBreakpoint=false,stopMask=0,stopLabel='';
let lastHighlight=null,budget=0,lastTime=performance.now(),loadId=0;
const experience=createExperience({seek:cycle=>action(()=>sim.seek(cycle)),advance:()=>action(()=>sim.tick(1)),watchEvent:()=>startEvent(128),inspectCache:(level,address)=>inspectors.inspectCache(level,address),inspectRam:address=>inspectors.inspectRam(address),pause:()=>action(()=>{}),finishWait:()=>startEvent(16),inspectCycle:kind=>action(()=>{if(kind==='cache'){$('cache-data-only').checked=state.access?.kind!=='fetch';inspectors.followMemoryStage(state.access);}else if(kind==='muldiv')inspectors.showMulDiv();else inspectors.showView(kind,true);})});
const inspectors=createInspectors({pause:()=>{pause();render();},nextCacheStage:dataOnly=>startEvent(dataOnly?32:16),nextMulDivCycle:()=>startEvent(64),onInspect:name=>experience.viewport.inspect(name),onSelectRegister:register=>experience.selectRegister(register),onSelectMemory:address=>experience.selectMemory(address)});

function showError(error){$('error').textContent=String(error);$('error').hidden=false;}
function pause(){running=false;temporaryBreakpoint=null;budget=0;}
function readState(){state=JSON.parse(sim.snapshot());}
function assembly(){
  $('assembly').innerHTML=rows.map(row=>`${row.symbol?`<div class="symbol-row">${escape(row.symbol)}:</div>`:''}<div class="asm-line" role="listitem" data-pc="${row.address}" title="${hex(row.raw)}"><button class="breakpoint ${breakpoints.has(row.address)?'set':''}" data-breakpoint="${row.address}" aria-label="Toggle breakpoint at ${hex(row.address)}" aria-pressed="${breakpoints.has(row.address)}">●</button><span class="asm-addr">${hex(row.address)}</span><span class="asm-op">${escape(row.text)}</span></div>`).join('');
  lastHighlight=null;
}
async function loadProgram(bytes,filename,source,description){
  const next=new Simulator(bytes);
  pause();if(sim)sim.free();sim=next;breakpoints.clear();stopLabel='';stopMask=0;
  $('error').hidden=true;$('filename').textContent=filename;$('source').textContent=source;$('program-hint').textContent=description;
  rows=JSON.parse(sim.program());readState();assembly();
  const data=state.symbols.find(s=>s.name==='numbers'||s.name==='values')||state.symbols.find(s=>s.kind===1);
  inspectors.reset(data?(data.address&~63)>>>0:0x80000000);experience.reset(state,sim);render();
}
async function loadExample(name){
  const id=++loadId;pause();
  try{
    const [binary,source]=await Promise.all([fetch(`programs/${name}.elf`),fetch(`programs/${name}.${name==='alu'?'S':'c'}`)]);
    if(!binary.ok||!source.ok)throw new Error('Example files are missing. Run python3 scripts/build.py.');
    const [bytes,text]=await Promise.all([binary.arrayBuffer(),source.text()]);
    if(id!==loadId)return;
    await loadProgram(new Uint8Array(bytes),`${name}.elf`,text,descriptions[name]);
  }catch(error){showError(error);}
}

function render(){
  if(!sim)return;
  readState();if(state.halted)pause();
  $('cycles').textContent=state.cycle.toLocaleString();$('retired').textContent=state.retired.toLocaleString();$('pc').textContent=hex(state.pc);
  const d=state.caches[1];$('hit-rate').textContent=d.hits+d.misses?`${Math.round(100*d.hits/(d.hits+d.misses))}%`:'—';
  $('status').textContent=state.fault?'Fault':state.halted?`Exited · ${state.exit_code}`:running?'Running':stopLabel?`Paused · ${stopLabel}`:'Paused';
  $('status-dot').style.background=state.fault?'#f58f82':running?'var(--a)':'var(--write)';
  $('play').textContent=running?'Pause':state.halted?'Finished':'Run';
  for(const id of ['play','cycle','step','next-event','cache-next','md-next'])$(id).disabled=state.halted;
  $('back').disabled=!state.can_back;
  const main=state.symbols.find(s=>s.name==='main');$('to-main').disabled=!main||state.halted||state.pc===main.address;
  if(state.fault)showError(state.fault);
  const currentPc=state.halted?(state.instruction?.pc??state.pc):state.pc;
  document.querySelectorAll('.asm-line.active').forEach(el=>el.classList.remove('active'));
  const row=$('assembly').querySelector(`[data-pc="${currentPc}"]`);
  if(row){
    row.classList.add('active');
    if($('follow').checked&&currentPc!==lastHighlight&&!$('assembly').hidden){
      const top=row.offsetTop-$('assembly').offsetTop;
      if(top<$('assembly').scrollTop+30||top>$('assembly').scrollTop+$('assembly').clientHeight-50)$('assembly').scrollTop=Math.max(0,top-100);
    }
  }
  lastHighlight=currentPc;
  const symbol=[...state.symbols].reverse().find(s=>s.kind===2&&s.address<=currentPc&&currentPc<s.address+s.size);
  $('symbol-label').textContent=symbol?.name||'machine code';
  const a=state.access;
  document.querySelectorAll('.memory-wire').forEach(el=>{
    const level=el.dataset.level;
    const visited=a&&(level==='L2I'?a.path.includes('L2')&&a.kind==='fetch':level==='L2D'?a.path.includes('L2')&&a.kind!=='fetch':a.path.includes(level));
    el.classList.toggle('active',Boolean(visited&&!a.complete&&a.stage_kind!=='fill'));
    el.classList.toggle('returning',Boolean(visited&&a.stage_kind==='fill'));
  });
  state.caches.forEach((cache,i)=>{
    $(`cache-label-${i}`).textContent=`${cache.lines.filter(l=>l.valid).length} / ${cache.lines.length} valid`;
    document.querySelector(`[data-cache="${i}"]`).classList.toggle('active',Boolean(a&&!a.complete&&a.stage_level===i));
  });
  $('ram-node').classList.toggle('active',Boolean(a&&!a.complete&&['read','write'].includes(a.stage_kind)));
  $('uart-unit').classList.toggle('active',Boolean(a&&a.path.includes('UART')));
  $('access-kind').textContent=a?a.kind.toUpperCase():'READY';
  $('access-info').textContent=a?`0x${hex(a.address)} · ${a.size} B · ${a.path.join(' → ')} · ${a.stage}`:'No memory access';
  $('wait-time').textContent=a?a.complete?`${a.latency} cycles · ${hex(a.value,a.size*2)}`:`${a.remaining} cycles left`:'';
  $('wait-bar').style.width=a?`${a.complete?100:100*(1-a.remaining/a.stage_latency)}%`:'0';
  if($('console').textContent!==state.console){$('console').textContent=state.console;$('console').scrollTop=$('console').scrollHeight;}
  inspectors.render(state,sim);
  experience.render(state,sim);
}

function start(mask=0){
  if(!sim||state.halted)return;
  experience.showClockEdge();
  running=true;skipBreakpoint=true;stopMask=mask;stopLabel='';budget=0;lastTime=performance.now();render();
}
function startEvent(mask){pause();start(mask);}
function toggleRun(){if(running){pause();render();}else start();}
function action(fn){if(!sim)return;pause();stopLabel='';$('error').hidden=true;fn();render();}
$('play').onclick=toggleRun;
$('cycle').onclick=()=>{action(()=>sim.tick(1));experience.replayClock();};$('step').onclick=()=>action(()=>sim.step());$('back').onclick=()=>action(()=>sim.back());
$('reset').onclick=()=>action(()=>{sim.reset();readState();inspectors.reset();experience.reset(state,sim);});
$('next-event').onclick=()=>startEvent(Number($('stop-event').value));
$('to-main').onclick=()=>{const main=state.symbols.find(s=>s.name==='main');if(main){temporaryBreakpoint=main.address;start();}};
$('example').onchange=e=>loadExample(e.target.value);
$('upload').onchange=async e=>{
  const file=e.target.files[0];if(!file)return;
  const id=++loadId;pause();
  try{
    if(file.size>32*1024*1024)throw new Error('ELF file exceeds the 32 MiB upload limit.');
    const bytes=new Uint8Array(await file.arrayBuffer());
    if(id!==loadId)return;
    await loadProgram(bytes,file.name,'Source unavailable for this executable.',file.name);
  }catch(error){showError(error);}
  e.target.value='';
};
document.querySelector('.upload').onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();$('upload').click();}};
$('assembly').onclick=e=>{const button=e.target.closest('[data-breakpoint]');if(!button)return;const pc=Number(button.dataset.breakpoint);breakpoints.has(pc)?breakpoints.delete(pc):breakpoints.add(pc);assembly();render();};
document.querySelectorAll('[data-code]').forEach(button=>button.onclick=()=>{
  const source=button.dataset.code==='source';$('source').hidden=!source;$('assembly').hidden=source;
  document.querySelectorAll('[data-code]').forEach(b=>b.classList.toggle('active',b===button));
});
$('speed').oninput=e=>$('speed-label').textContent=Number(e.target.value)===0?'1 clock / 2 s · signals':`${speeds[e.target.value].toLocaleString()} clocks/s · edge states`;
$('input-form').onsubmit=e=>{e.preventDefault();if(!sim||state.halted)return;sim.input($('terminal-input').value+'\n');$('terminal-input').value='';if(!running)start();};
$('help').onclick=()=>$('help-dialog').showModal();$('close-help').onclick=()=>$('help-dialog').close();
window.addEventListener('keydown',e=>{
  if(/INPUT|SELECT|TEXTAREA|BUTTON/.test(e.target.tagName)||$('help-dialog').open||!sim)return;
  if(e.code==='Space'){e.preventDefault();toggleRun();}
  if(e.key==='ArrowRight'){e.preventDefault();$('step').click();}
  if(e.key==='ArrowLeft'){e.preventDefault();$('back').click();}
  if(e.key.toLowerCase()==='r')$('reset').click();
});

function frame(time){
  const dt=Math.max(0,Math.min(time-lastTime,100));lastTime=time;
  if(running&&sim){
    budget+=dt/1000*((temporaryBreakpoint!=null||stopMask!==0)?40000:speeds[$('speed').value]);
    const count=Math.min(Math.floor(budget),10000);
    if(count>0){
      const stops=new Uint32Array([...breakpoints,...(temporaryBreakpoint==null?[]:[temporaryBreakpoint])]);
      const ran=sim.run_until(count,stops,skipBreakpoint,stopMask);skipBreakpoint=false;budget-=ran;
      const reason=sim.stop_reason();
      if(reason||ran<count){pause();stopLabel=reason==='breakpoint'?'':reason;}
      render();
      if(running&&!stopMask&&temporaryBreakpoint==null&&Number($('speed').value)===0&&ran===1)experience.replayClock(false);
      const open=$('auto-inspect').checked;
      if(reason && ['cache stage','cache miss','cache fill'].includes(reason))inspectors.followMemoryStage(state.access,open);
      if(reason==='ALU result')inspectors.showView('alu',open);
      if(reason==='multiply/divide cycle')inspectors.showMulDiv(open);
      if(reason==='watched value'){
        const d=state.datapath;
        if(d?.rd!=null){$('wave-register').value=String(d.rd);experience.render(state,sim);}
        if(d?.alu?.unit==='muldiv')inspectors.showMulDiv(open);
        else if(state.access?.kind!=='fetch')inspectors.followMemoryStage(state.access,open);
        else inspectors.showView('alu',open);
      }
    }
  }
  inspectors.animate(time);experience.animate(time);requestAnimationFrame(frame);
}
try{
  const module=await import('./pkg/visual_cpu.js');await module.default();Simulator=module.Simulator;
  await loadExample('flow');requestAnimationFrame(frame);
}catch(error){showError(`Could not start the engine: ${error}. Run python3 scripts/build.py and serve the web directory over HTTP.`);$('status').textContent='Engine unavailable';}
