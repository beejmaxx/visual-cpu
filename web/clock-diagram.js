import {$,hex,names} from './inspectors.js';
import {clockWrites} from './clock-flow.js';

const components={pc:'#pc-node',instruction:'#instruction-node',decoder:'#decoder-node',registers:'#register-bank',alu:'#alu-node',muldiv:'#muldiv-node',memory:'#load-store-node',write:'#write-node','operand-a':'.a-node','operand-b':'.b-node',L1I:'[data-cache="0"]',L1D:'[data-cache="1"]',L2:'[data-cache="2"]',L3:'[data-cache="3"]',RAM:'#ram-node',UART:'#uart-unit'};
const display=n=>n==null?'—':hex(n);

export function drawClock(state,step,flow){
  const detail=state.cycle_detail,diagram=$('diagram'),p=state.datapath;
  diagram.classList.add('cycle-present');diagram.dataset.waiting=String(detail.waiting);diagram.dataset.clockStep=String(step);
  diagram.querySelectorAll('.cycle-active,.clock-input,.clock-work,.clock-latch').forEach(el=>el.classList.remove('cycle-active','clock-input','clock-work','clock-latch'));
  // A queued next stage must not look as if it ran on the selected clock.
  diagram.querySelectorAll('[data-cache],#ram-node,#uart-unit').forEach(el=>el.classList.remove('active'));
  const group=step===0?flow.sources:step===1?flow.work:flow.destinations;
  for(const component of group){const el=diagram.querySelector(components[component]||'.absent');el?.classList.add('cycle-active',['clock-input','clock-work','clock-latch'][step]);}
  diagram.querySelectorAll('.signal').forEach(el=>el.classList.remove('cycle-signal','clock-input','clock-work','clock-latch'));
  const wires=step===0?flow.inputWires:step===1?flow.workWires:flow.edgeWires;
  wires.forEach(id=>$(id)?.classList.add('cycle-signal',['clock-input','clock-work','clock-latch'][step]));
  diagram.querySelectorAll('.memory-wire').forEach(el=>{
    el.classList.remove('active','returning','clock-held-wire');
    if(el.dataset.level===flow.memoryWire){
      if(detail.waiting)el.classList.add('clock-held-wire');
      else if(step>0)el.classList.add(flow.returning?'returning':'active');
    }
  });
  const before=detail.before,after=detail.after;
  if(!before||!after)return;
  const view=step===2?after:before;
  $('fetch-detail').textContent=hex(view.pc);$('instruction-word').textContent=display(view.instruction_word);
  // A computed combinational value may appear in Logic, but architectural
  // registers and cache valid bits remain at their before-edge values.
  const evaluated=step>=1?after:before;
  $('operand-a').textContent=display(evaluated.operand_a);$('operand-b').textContent=display(evaluated.operand_b);
  $('execute-detail').textContent=display(evaluated.alu_result);
  $('write-value').textContent=display(evaluated.write_value);
  $('memory-detail').textContent=evaluated.memory_address==null?'no data access':hex(evaluated.memory_address);
  $('memory-value').textContent=evaluated.memory_address==null?'—':p?.store_data!=null?`store ${hex(p.store_data)}`:evaluated.write_value!=null?`load ${hex(evaluated.write_value)}`:'load pending';
  if(detail.phase==='Decode'&&step===0){
    $('decode-detail').textContent='decode not evaluated';$('operand-a-source').textContent='A LATCH · HELD';$('operand-b-source').textContent='B LATCH · HELD';
    $('read-a-label').textContent='Read port A not selected';$('read-b-label').textContent='Read port B not selected';
  }else if(p){
    $('decode-detail').textContent=`op ${hex(p.raw&127,2)} · f3 ${(p.raw>>>12)&7} · f7 ${hex(p.raw>>>25,2)}`;
    $('operand-a-source').textContent=`A ← ${p.left_source}`;$('operand-b-source').textContent=`B ← ${p.right_source}`;
    $('read-a-label').textContent=p.read_a?`port A · x${p.read_a.register} = ${hex(p.read_a.value)}`:'port A · unused';
    $('read-b-label').textContent=p.read_b?`port B · x${p.read_b.register} = ${hex(p.read_b.value)}`:'port B · unused';
  }
  if(detail.phase==='Execute'&&step===0){$('next-pc').textContent='pending';$('muldiv-value').textContent=p?.alu?.muldiv?'before this iteration':'inactive';}
  else{$('next-pc').textContent=p?.next_pc==null?'—':hex(p.next_pc);const a=p?.alu;$('muldiv-value').textContent=a?.muldiv?(a.ready?`${a.function} → ${hex(a.result)}`:`${a.function} · cycle ${a.muldiv.execute_cycle}/34`):'inactive';}
  const writes=clockWrites(state);
  $('write-enable').textContent=`WE = ${writes&&step>0?1:0}${writes&&step<2?' · edge pending':''}`;
  const format=$('register-format').value;
  diagram.querySelectorAll('[data-register]').forEach(el=>{
    const i=Number(el.dataset.register),n=view.regs[i];
    el.lastElementChild.textContent=format==='hex'?hex(n):format==='signed'?String(n|0):String(n);
    el.classList.toggle('changed',writes&&step===2&&state.last_written===i);
    el.classList.toggle('read-a',detail.phase==='Decode'&&step>0&&p?.read_a?.register===i);
    el.classList.toggle('read-b',detail.phase==='Decode'&&step>0&&p?.read_b?.register===i);
    el.title=`x${i} / ${names[i]} · ${n} · 0x${hex(n)} · ${step<2?'before':'after'} clock ${state.cycle}`;
  });
  const selected=Number($('selected-register').textContent.match(/^x(\d+)/)?.[1]??10),n=view.regs[selected];
  $('register-value').textContent=hex(n);$('register-decimal').textContent=`unsigned ${n} · signed ${n|0}`;
  [...$('register-bits').children].forEach((el,i)=>{const bit=(n>>>(31-i))&1;el.classList.toggle('one',Boolean(bit));el.lastChild.textContent=String(bit);});
  state.caches.forEach((cache,i)=>{$(`cache-label-${i}`).textContent=`${view.cache_valid[i]} / ${cache.lines.length} valid`;});
  diagram.querySelectorAll('[data-unit]').forEach(el=>el.classList.toggle('selected',detail.phase==='Execute'&&step>0&&el.dataset.unit===p?.alu?.unit));
}
