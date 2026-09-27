import {createMulDivInspector} from './muldiv-inspector.js';
export const $ = id => document.getElementById(id);
export const hex = (value, width = 8) => (value >>> 0).toString(16).padStart(width, '0');
export const bits = (value, width = 32) => (value >>> 0).toString(2).padStart(width, '0');
export const escape = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export const names = ['zero','ra','sp','gp','tp','t0','t1','t2','s0','s1','a0','a1','a2','a3','a4','a5','a6','a7','s2','s3','s4','s5','s6','s7','s8','s9','s10','s11','t3','t4','t5','t6'];
const descending = Array.from({length:32}, (_, i) => 31-i);
const unitNames = {adder:'Adder / subtractor',logic:'Bitwise logic',shifter:'Barrel shifter',compare:'Comparator',muldiv:'Multiply / divide'};
const text = (id, value) => {$(id).textContent = value;};
const signal = (id, active) => $(id).classList.toggle('on', Boolean(active));
const binaryGroups = value => bits(value).match(/.{8}/g).join(' ');

function bytesHTML(bytes, address, access) {
  let html = '';
  for (let row=0;row<bytes.length;row+=8) {
    html += `<div class="memory-row"><span class="addr">${hex(address+row)}</span>`;
    for (let col=0;col<8 && row+col<bytes.length;col++) {
      const at=address+row+col, byte=bytes[row+col];
      const touched=access && at>=access.address && at<access.address+access.size;
      html += `<span class="byte ${byte?'nonzero':''} ${touched?(access.kind==='store'?'written':'touched'):''}" data-memory-byte="${at}" role="button" tabindex="0" aria-label="Trace byte at ${hex(at)}" title="0x${hex(at)} · offset ${row+col} · decimal ${byte}">${hex(byte,2)}</span>`;
    }
    html+='</div>';
  }
  return html;
}

export function createInspectors({pause,nextCacheStage,nextMulDivCycle,onInspect,onSelectRegister,onSelectMemory}) {
  let state, sim, selectedRegister=10, selectedBit=0, selectedLine=0, unit='adder';
  let activeView='alu', memoryAddress=0x80000000, previousData=null;
  let traceKey='', reveal=32, animating=false, lastReveal=0;
  let registerKey='', renderedAluKey='';
  const muldivInspector=createMulDivInspector({nextCycle:()=>{unit='muldiv';$('follow-unit').checked=true;nextMulDivCycle();}});

  function showView(name, scroll=false) {
    activeView=name;
    document.querySelectorAll('[data-inspector]').forEach(button=>{
      const active=button.dataset.inspector===name;
      button.classList.toggle('active',active);button.setAttribute('aria-selected',String(active));
    });
    ['alu','cache','ram','decode'].forEach(view=>$(`view-${view}`).hidden=view!==name);
    if(state)renderInspection();
    if(scroll)onInspect?.(name==='alu'&&unit==='muldiv'?'muldiv':name);
  }

  function chooseUnit(name, scroll=false) {
    unit=name;$('follow-unit').checked=false;showView('alu',scroll);
  }

  function renderRegisters() {
    const d=state.datapath;
    if($('follow-register').checked && state.last_written!=null) selectedRegister=state.last_written;
    const format=$('register-format').value;
    const key=`${state.regs.join(',')}/${d?.read_a?.register}/${d?.read_b?.register}/${state.last_written}/${selectedRegister}/${format}`;
    if(key===registerKey)return;registerKey=key;
    $('registers').innerHTML=state.regs.map((value,i)=>{
      const display=format==='hex'?hex(value):format==='signed'?String(value|0):String(value);
      const classes=[d?.read_a?.register===i?'read-a':'',d?.read_b?.register===i?'read-b':'',state.last_written===i?'changed':'',selectedRegister===i?'selected':''].join(' ');
      return `<button class="register ${classes}" data-register="${i}" aria-label="Inspect x${i} ${names[i]}" aria-pressed="${selectedRegister===i}" title="x${i} / ${names[i]} · hex ${hex(value)} · unsigned ${value} · signed ${value|0}${i===0?' · hardwired zero':''}"><span class="register-name">${names[i]}<span class="register-index">x${i}</span></span><span>${display}</span></button>`;
    }).join('');
    const value=state.regs[selectedRegister];
    text('selected-register',`x${selectedRegister} / ${names[selectedRegister]}${selectedRegister===0?' · hardwired zero':''}`);
    text('register-value',hex(value));text('register-decimal',`unsigned ${value} · signed ${value|0}`);
    $('register-bits').innerHTML=descending.map(bit=>`<span class="register-bit ${(value>>>bit)&1?'one':''} ${bit%8===7?'byte-start':''}" title="Bit ${bit}"><small>${bit}</small>${(value>>>bit)&1}</span>`).join('');
  }

  function renderCpu() {
    const d=state.datapath, a=d?.alu;
    const pulse=d?.committed && d.write_enable && state.last_written===d.rd;
    text('phase-caption',state.halted?'Halted':`Next phase: ${state.phase}`);
    document.querySelectorAll('[data-phase]').forEach(el=>el.classList.toggle('active',!state.halted && el.dataset.phase===state.phase));
    text('trace-status',d?(d.committed?'Last committed':a?.ready===false?`Execute ${a.muldiv.execute_cycle}/34`:a?'Executed / pending':'Operands latched'):'No decoded instruction');
    text('current-instruction',d?`0x${hex(d.pc)}  ${d.instruction}`:'—');
    text('fetch-detail',hex(state.pc));text('instruction-word',state.instruction?hex(state.instruction.raw):'—');
    text('decode-detail',d?`op ${hex(d.raw&127,2)} · f3 ${(d.raw>>>12)&7} · f7 ${hex(d.raw>>>25,2)}`:'—');
    text('next-pc',d?.next_pc!=null?hex(d.next_pc):'—');
    text('operand-a-source',d?`A ← ${d.left_source}`:'A SELECT');
    text('operand-b-source',d?`B ← ${d.right_source}`:'B SELECT');
    text('operand-a',d?hex(d.left):'—');text('operand-b',d?hex(d.right):'—');
    text('read-a-label',d?.read_a?`port A · x${d.read_a.register} = ${hex(d.read_a.value)}`:'port A · unused');
    text('read-b-label',d?.read_b?`port B · x${d.read_b.register} = ${hex(d.read_b.value)}`:'port B · unused');
    text('alu-function',a?a.function.toUpperCase():'—');
    text('execute-detail',a && a.unit!=='muldiv'?hex(a.result):'—');
    text('muldiv-value',a?.muldiv?(a.ready?`${a.function} → ${hex(a.result)}`:`${a.function} · cycle ${a.muldiv.execute_cycle}/34`):'inactive');
    text('write-register',d?.rd!=null?`x${d.rd} / ${names[d.rd]}`:'no destination');
    text('write-value',d?.write_value!=null?hex(d.write_value):'—');
    text('write-enable',`WE = ${pulse?1:0}${d?.rd===0?' · x0 discard':d?.write_enable&&!d.committed?' · pending':''}`);
    text('write-source',`source: ${d?.write_source||'—'}`);
    text('write-detail',d?.write_enable?`${names[d.rd]}: ${hex(d.before)} → ${d.write_value!=null?hex(d.write_value):'pending'}`:'rd / WDATA');
    text('memory-detail',d?.memory_address!=null?hex(d.memory_address):'no data access');
    text('memory-value',d?.memory_address==null?'—':d.store_data!=null?`store ${hex(d.store_data)}`:d.write_value!=null?`load ${hex(d.write_value)}`:'load pending');
    text('memory-route',d?.memory_address>=0x10000000 && d.memory_address<0x1000000c?'UART · uncached':'L1D → memory hierarchy');
    document.querySelectorAll('#diagram [data-unit]').forEach(el=>el.classList.toggle('selected',el.dataset.unit===a?.unit));
    signal('wire-read-a',d?.read_a);signal('wire-read-b',d?.read_b);
    signal('wire-alu-a',a && a.unit!=='muldiv');signal('wire-alu-b',a && a.unit!=='muldiv');
    signal('wire-muldiv-a',a?.unit==='muldiv');signal('wire-muldiv-b',a?.unit==='muldiv');signal('wire-muldiv-result',a?.unit==='muldiv' && a.ready);
    signal('wire-result',a && a.unit!=='muldiv');signal('wire-write-data',d?.write_value!=null);signal('wire-writeback',pulse);
    signal('wire-address',d?.memory_address!=null);signal('wire-store',d?.store_data!=null);
    signal('wire-load',d?.write_source==='memory' && d.write_value!=null);
    signal('wire-pc-a',d?.left_source==='PC');signal('wire-immediate',d?.right_source==='immediate');
    renderRegisters();
  }

  function bitMatrix(rows, selected=true) {
    let html='<span class="row-label">bit</span>'+descending.map(bit=>`<button class="bit-head ${selected&&bit===selectedBit?'chosen':''} ${bit%8===7?'byte-start':''}" data-bit="${bit}" aria-label="Inspect bit ${bit}">${bit}</button>`).join('');
    for(const row of rows) {
      html+=`<span class="row-label">${escape(row.name)}</span>`;
      html+=descending.map(bit=>{
        const unknown=row.known && !row.known(bit);
        const value=unknown?'·':row.value(bit);
        return `<span class="bit-cell ${value===1?'one':''} ${row.className||''} ${unknown?'unknown':''} ${selected&&bit===selectedBit?'chosen':''} ${bit%8===7?'byte-start':''}" data-bit="${bit}" title="${escape(row.name)}[${bit}] = ${unknown?'not revealed':value}">${value}</span>`;
      }).join('');
    }
    return html;
  }

  function renderAdder(a) {
    const adder=a.adder;
    text('adder-summary',`${adder.subtract?'A + NOT(B) + 1':'A + B + 0'} · C32=${adder.carry_out} · signed overflow=${Number(adder.overflow)}`);
    const at=key=>bit=>adder.bits[bit][key];
    $('adder-bits').innerHTML=bitMatrix([
      {name:'A',value:at('a')},{name:'B',value:at('b')},{name:adder.subtract?'B′ = NOT B':'B′ = B',value:at('effective_b')},
      {name:'Carry in',value:at('carry_in'),className:'carry',known:bit=>bit<=reveal},
      {name:'A XOR B′',value:at('propagate')},{name:'A AND B′',value:at('generate')},
      {name:'Sum',value:at('sum'),className:'sum',known:bit=>bit<reveal},
      {name:'Carry out',value:at('carry_out'),className:'carry',known:bit=>bit<reveal},
    ]);
    const b=adder.bits[selectedBit];
    text('bit-title',`Full adder · bit ${selectedBit}`);
    text('bit-equation',`${b.a} + ${b.effective_b} + Cin(${b.carry_in}) = ${b.sum} + 2 × Cout(${b.carry_out})`);
    text('gate-a',`A=${b.a}`);text('gate-b',`B′=${b.effective_b}`);text('gate-cin',`Cin=${b.carry_in}`);
    text('gate-p',b.propagate);text('gate-g',b.generate);text('gate-h',b.propagate & b.carry_in);
    text('gate-sum',b.sum);text('gate-cout',b.carry_out);text('gate-sum-label',`SUM=${b.sum}`);text('gate-carry-label',`Cout=${b.carry_out}`);
    document.querySelectorAll('[data-gate-signal]').forEach(el=>{
      const key=el.dataset.gateSignal;
      el.classList.toggle('one',Boolean(key==='carry_term'?(b.propagate & b.carry_in):b[key]));
    });
    $('bit-prev').disabled=selectedBit===0;$('bit-next').disabled=selectedBit===31;
    text('propagate',animating?'Revealing…':'Reveal carries');
  }

  const cards=entries=>entries.map(([label,value])=>`<div class="signal-card">${escape(label)}<strong>${escape(value)}</strong></div>`).join('');

  function renderAlu() {
    const d=state.datapath, a=d?.alu;
    const key=d?`${d.pc}/${d.left}/${d.right}/${a?.result}/${d.committed}/${a?.muldiv?.execute_cycle}`:'';
    if(key!==traceKey){traceKey=key;reveal=32;animating=false;}
    if($('follow-unit').checked && a) unit=a.unit;
    text('alu-heading',unitNames[unit]);
    text('alu-detail',a?`${d.instruction} · 0x${hex(d.pc)}${d.committed?' · committed':''}`:'No ALU result');
    $('alu-empty').hidden=Boolean(a)||unit==='muldiv';$('alu-content').hidden=!a&&unit!=='muldiv';
    document.querySelectorAll('[data-alu-view]').forEach(el=>{
      el.classList.toggle('active',el.dataset.aluView===unit);el.classList.toggle('selected',el.dataset.aluView===a?.unit);
    });
    ['adder','logic','shifter','compare','muldiv'].forEach(name=>$(`${name}-view`).hidden=name!==unit);
    if(!a){renderedAluKey='';for(const id of ['alu-a','alu-b','alu-select','alu-output'])text(id,'—');if(unit==='muldiv')muldivInspector.render(null);return;}
    text('alu-a',hex(a.left));text('alu-b',hex(a.right));text('alu-select',a.function.toUpperCase());text('alu-output',a.ready?hex(a.result):'pending');
    const renderKey=`${key}/${unit}`;
    if(renderedAluKey===renderKey)return;renderedAluKey=renderKey;
    if(unit==='adder')renderAdder(a);
    if(unit==='logic') {
      $('logic-bits').innerHTML=bitMatrix([
        {name:'A',value:bit=>(a.left>>>bit)&1},{name:'B',value:bit=>(a.right>>>bit)&1},
        ...['and','or','xor'].map(op=>({name:op.toUpperCase()+(a.function===op?' ←':''),value:bit=>(a[op]>>>bit)&1,className:a.function===op?'sum':''})),
      ],false);
    }
    if(unit==='shifter') {
      const s=a.shifter;
      text('shift-summary',`${s.arithmetic?'Arithmetic':'Logical'} ${s.direction} · amount = B[4:0] = ${bits(s.amount,5)} (${s.amount})${a.unit!=='shifter'?' · unit not selected for this instruction':''}`);
      $('shift-stages').innerHTML=s.stages.map((stage,i)=>`<tr><td>Shift ${stage.distance}</td><td><span class="${stage.enabled?'enabled':''}">B[${i}] = ${Number(stage.enabled)} · ${stage.enabled?'shift':'bypass'}</span></td><td><code>${hex(stage.input)}</code><span class="binary-value">${binaryGroups(stage.input)}</span></td><td><code>${hex(stage.output)}</code><span class="binary-value">${binaryGroups(stage.output)}</span></td></tr>`).join('');
    }
    if(unit==='compare') {
      const c=a.compare;
      $('compare-signals').innerHTML=cards([['A sign bit',Number(c.a_negative)],['B sign bit',Number(c.b_negative)],['A == B',Number(c.equal)],['A < B (signed)',Number(c.signed_less)],['A < B (unsigned)',Number(c.unsigned_less)],['Selected predicate',a.unit==='compare'?`${a.function} = ${a.result}`:'not selected']]);
    }
    if(unit==='muldiv')muldivInspector.render(a.muldiv);
  }

  function renderCache() {
    const level=Number($('cache-select').value), cache=state.caches[level];
    const access=state.cache_accesses[level];
    const resolved=access?.probes.find(p=>p.level===level);
    const probe=resolved||(access?.pending_probe?.level===level?access.pending_probe:null);
    if(probe && $('follow-cache').checked)selectedLine=probe.index;
    selectedLine=Math.min(selectedLine,cache.lines.length-1);
    const indexBits=Math.log2(cache.lines.length);
    text('cache-geometry',`${cache.lines.length} sets · 1 way · 64 B / line · ${cache.lines.length*64} B`);
    $('cache-stats').innerHTML=[['hits',cache.hits],['misses',cache.misses],['fills',cache.fills],['evictions',cache.evictions]].map(([name,value])=>`<span><strong>${value}</strong> ${name}</span>`).join('');
    text('lookup-context',access?`Request @${access.cycle} · ${probe?.cycle!=null?'lookup @'+probe.cycle:'lookup pending'} · PC ${hex(access.pc)} · ${access.instruction} · ${access.kind} 0x${hex(access.address)} (${access.size} B)`:'No lookup recorded for this cache');
    const clock=state.cycle_detail,clockMemory=clock?.memory_clock;
    const current=clockMemory&&state.access?.cycle===access?.cycle;
    text('cache-stage-label',current?`Clock ${state.cycle}: ${clock.title} · Next: ${access.complete?'Complete':`${access.stage} · ${access.remaining} clocks`}`:access?`Recorded request: ${access.stage}${access.complete?` · ${access.latency} cycles`:` · ${access.remaining} cycles left`}`:'No request');
    $('cache-journey').innerHTML=access?access.steps.map(step=>`<span class="journey-step" title="${escape(step.text)}">@${step.cycle} ${step.level!=null?state.caches[step.level].name+' ':''}${escape(step.kind.replace('cache_','').replaceAll('_',' '))}</span>`).join(''):'';
    $('address-split').innerHTML=probe?[
      {label:`TAG [31:${indexBits+6}]`,width:26-indexBits,value:probe.tag,color:'tag'},
      {label:`INDEX [${indexBits+5}:6]`,width:indexBits,value:probe.index,color:'index'},
      {label:'OFFSET [5:0]',width:6,value:probe.offset,color:'offset'},
    ].map(field=>`<div class="address-field ${field.color}" style="flex:${field.width}"><small>${field.label}</small><code>${bits(field.value,field.width)}</code><span>${field.color==='index'?`set ${field.value}`:field.color==='offset'?`byte ${field.value}`:`0x${hex(field.value,Math.ceil(field.width/4))}`}</span></div>`).join(''):'';
    $('tag-compare').innerHTML=probe?`<span>At lookup: V=<code>${Number(probe.valid)}</code></span><span>stored <code>${probe.stored_tag==null?'—':hex(probe.stored_tag,6)}</code> ${probe.valid&&probe.stored_tag===probe.tag?'=':'≠'} requested <code>${hex(probe.tag,6)}</code></span><strong class="outcome ${resolved?(probe.hit?'hit':'miss'):''}">${resolved?(probe.hit?'HIT':'MISS'):'LOOKUP PENDING'}</strong><span>${access.path.map(escape).join(' → ')}</span><span>${access.read_value!=null?`read ${hex(access.read_value,access.size*2)}`:'data pending'}${access.kind==='store'?` · store ${hex(access.value,access.size*2)} · write-through to RAM`:''}</span>`:'';
    text('circuit-index',probe?`set ${probe.index}`:'—');text('circuit-tag',probe?.stored_tag!=null?hex(probe.stored_tag,6):'—');
    text('circuit-valid',`V = ${probe?Number(probe.valid):'—'}`);
    text('circuit-compare',probe?.valid?`${hex(probe.stored_tag^probe.tag,6)}`:'—');
    text('circuit-hit',resolved?(probe.hit?'HIT = 1':'HIT = 0'):'pending');
    text('circuit-data',access?.read_value!=null?hex(access.read_value,access.size*2):'pending');
    text('circuit-offset',probe?`bytes ${probe.offset}–${probe.offset+access.size-1}${access.data_ready&&!probe.hit?' · lower-level return':''}`:'offset —');
    const tagBits=26-indexBits, indices=Array.from({length:tagBits},(_,i)=>tagBits-1-i);
    $('tag-bits').style.gridTemplateColumns=`76px repeat(${tagBits}, minmax(18px,1fr))`;
    $('tag-bits').innerHTML=probe?'<span class="row-label">tag bit</span>'+indices.map(bit=>`<span class="bit-head">${bit}</span>`).join('')+[
      ['Requested',probe.tag],['Stored',probe.valid?probe.stored_tag:null],['XOR',probe.valid?(probe.tag^probe.stored_tag):null],
    ].map(([name,value])=>`<span class="row-label">${name}</span>`+indices.map(bit=>`<span class="bit-cell ${value!=null&&((value>>>bit)&1)?'one':''}">${value==null?'·':(value>>>bit)&1}</span>`).join('')).join(''):'';
    $('cache-lines').innerHTML=cache.lines.map((line,index)=>`<tr class="${line.valid?'valid':''} ${index===selectedLine?'selected':''} ${probe?.index===index?'lookup':''}"><td><button data-line="${index}" aria-label="Inspect set ${index}" aria-pressed="${index===selectedLine}">${index}</button></td><td>${Number(line.valid)}</td><td>${line.valid?hex(line.address>>>(6+indexBits),6):'—'}</td><td>${line.valid?hex(line.address):'—'}</td><td>${line.valid?line.data.slice(0,8).map(b=>hex(b,2)).join(' '):'—'}</td></tr>`).join('');
    const line=cache.lines[selectedLine];
    text('line-title',`Current set ${selectedLine} · ${line.valid?'0x'+hex(line.address):'invalid'}`);
    $('line-bytes').innerHTML=line.valid?bytesHTML(line.data,line.address,access):'<div class="empty">No valid line</div>';
    text('line-note',line.valid?`64 bytes · tag 0x${hex(line.address>>>(6+indexBits),6)} · clean (write-through)`:'The valid bit is 0.');
  }

  function renderRam() {
    const bytes=sim.memory(memoryAddress,Math.min(128,0x81000000-memoryAddress));
    $('memory-address').value=hex(memoryAddress);
    $('memory-bytes').innerHTML=bytesHTML(bytes,memoryAddress,state.cache_accesses[1]);
    text('memory-note',bytes.length?'Inspection does not change cache state.':'Outside RAM: 80000000–80ffffff.');
  }

  function renderDecode(){
    const d=state.datapath;
    text('decode-context',d?`${d.instruction} · 0x${hex(d.pc)} · operands latched at decode`:'No decoded instruction');
    if(!d){$('instruction-fields').innerHTML='';$('decode-signals').innerHTML='';text('decoded-immediate','—');return;}
    const op=d.raw&127;
    let format='I',fields=[['imm[11:0]',31,20],['rs1',19,15],['funct3',14,12],['rd',11,7],['opcode',6,0]];
    if(op===0x33){format='R';fields=[['funct7',31,25],['rs2',24,20],['rs1',19,15],['funct3',14,12],['rd',11,7],['opcode',6,0]];}
    if(op===0x23){format='S';fields=[['imm[11:5]',31,25],['rs2',24,20],['rs1',19,15],['funct3',14,12],['imm[4:0]',11,7],['opcode',6,0]];}
    if(op===0x63){format='B';fields=[['imm[12]',31,31],['imm[10:5]',30,25],['rs2',24,20],['rs1',19,15],['funct3',14,12],['imm[4:1]',11,8],['imm[11]',7,7],['opcode',6,0]];}
    if(op===0x37||op===0x17){format='U';fields=[['imm[31:12]',31,12],['rd',11,7],['opcode',6,0]];}
    if(op===0x6f){format='J';fields=[['imm[20]',31,31],['imm[10:1]',30,21],['imm[11]',20,20],['imm[19:12]',19,12],['rd',11,7],['opcode',6,0]];}
    text('decode-format',`${format} format · 0x${hex(d.raw)}`);
    $('instruction-fields').innerHTML=fields.map(([name,hi,lo])=>{
      const width=hi-lo+1,value=(d.raw>>>lo)&(2**width-1);
      return `<div class="instruction-field" style="flex:${width}"><small>[${hi}:${lo}]</small><strong>${name}</strong><code>${bits(value,width)}</code><span>${name.startsWith('rs')||name==='rd'?`x${value} / ${names[value]}`:'0x'+hex(value,Math.ceil(width/4))}</span></div>`;
    }).join('');
    const usesImmediate=format!=='R'&&!['ecall','ebreak','fence'].includes(d.operation);
    text('decoded-immediate',usesImmediate?`Decoded immediate / shift amount: ${d.immediate} · 0x${hex(d.immediate)} · ${binaryGroups(d.immediate)}`:'No immediate operand');
    $('decode-signals').innerHTML=cards([['Read port A',d.read_a?`x${d.read_a.register} = ${hex(d.read_a.value)}`:'unused'],['Read port B',d.read_b?`x${d.read_b.register} = ${hex(d.read_b.value)}`:'unused'],['ALU input A',d.left_source],['ALU input B',d.right_source],['ALU select',d.function||'none'],['Writeback select',d.write_source],['Destination',d.rd!=null?`x${d.rd} / ${names[d.rd]}`:'none'],['Register write enabled',Number(d.write_enable)],['Memory operation',d.store_data!=null?'store':d.write_source==='memory'?'load':'none']]);
  }

  function renderInspection(){if(activeView==='alu')renderAlu();else if(activeView==='cache')renderCache();else if(activeView==='decode')renderDecode();else renderRam();}

  function renderHistory() {
    const filter=$('event-filter').value;
    const events=state.log.filter(e=>filter==='all'||e.kind===filter).slice().reverse();
    $('event-list').innerHTML=events.map(e=>`<tr data-event="${escape(e.kind)}"><td>${e.cycle}</td><td>${hex(e.pc)}</td><td>${escape(e.text)}</td></tr>`).join('')||'<tr><td colspan="3">No events</td></tr>';
  }

  function render(nextState,nextSim) {
    state=nextState;sim=nextSim;renderCpu();
    if($('follow-memory').checked && state.last_data!=null && state.last_data!==previousData && state.last_data>=0x80000000 && state.last_data<0x81000000)memoryAddress=(state.last_data&~63)>>>0;
    previousData=state.last_data;
    renderInspection();renderHistory();
  }

  document.querySelectorAll('[data-inspector]').forEach(button=>button.onclick=()=>showView(button.dataset.inspector));
  document.querySelectorAll('[data-alu-view]').forEach(button=>button.onclick=()=>chooseUnit(button.dataset.aluView));
  document.querySelectorAll('#diagram [data-unit]').forEach(el=>el.onclick=e=>{e.stopPropagation();chooseUnit(el.dataset.unit,true);});
  function activate(el,fn){el.onclick=fn;el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();fn();}};}
  activate($('alu-node'),()=>{$('follow-unit').checked=true;showView('alu',true);});
  activate($('muldiv-node'),()=>chooseUnit('muldiv',true));
  activate($('ram-node'),()=>showView('ram',true));
  activate($('decoder-node'),()=>showView('decode',true));
  document.querySelectorAll('[data-cache]').forEach(el=>activate(el,()=>{$('cache-select').value=el.dataset.cache;$('follow-cache').checked=true;selectedLine=0;showView('cache',true);}));
  $('cache-select').onchange=()=>{selectedLine=0;if(state)renderCache();};
  $('cache-next').onclick=()=>nextCacheStage($('cache-data-only').checked);
  $('follow-cache').onchange=()=>{if(state)renderCache();};
  $('cache-lines').onclick=e=>{const button=e.target.closest('[data-line]');if(button){selectedLine=Number(button.dataset.line);$('follow-cache').checked=false;renderCache();}};
  $('registers').onclick=e=>{const button=e.target.closest('[data-register]');if(button){selectedRegister=Number(button.dataset.register);$('follow-register').checked=false;renderRegisters();onSelectRegister?.(selectedRegister);}};
  for(const id of ['line-bytes','memory-bytes']){
    const selectByte=e=>{const el=e.target.closest('[data-memory-byte]');if(el)onSelectMemory?.(Number(el.dataset.memoryByte));};
    $(id).onclick=selectByte;$(id).onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();selectByte(e);}};
  }
  $('register-format').onchange=()=>{if(state)renderRegisters();};$('follow-register').onchange=()=>{if(state)renderRegisters();};
  $('memory-address').onchange=e=>{const value=e.target.value.replace(/^0x/,'');if(/^[\da-f]{1,8}$/i.test(value)){memoryAddress=parseInt(value,16);$('follow-memory').checked=false;if(state)renderRam();}};
  $('follow-unit').onchange=()=>{if(state)renderAlu();};
  $('adder-bits').onclick=e=>{const bit=e.target.closest('[data-bit]');if(bit){selectedBit=Number(bit.dataset.bit);if(state?.datapath?.alu)renderAdder(state.datapath.alu);}};
  $('bit-prev').onclick=()=>{selectedBit=Math.max(0,selectedBit-1);renderAdder(state.datapath.alu);};
  $('bit-next').onclick=()=>{selectedBit=Math.min(31,selectedBit+1);renderAdder(state.datapath.alu);};
  $('propagate').onclick=()=>{if(!state?.datapath?.alu)return;pause();reveal=0;selectedBit=0;animating=true;lastReveal=performance.now();renderAdder(state.datapath.alu);};
  $('show-carries').onclick=()=>{animating=false;reveal=32;if(state?.datapath?.alu)renderAdder(state.datapath.alu);};
  $('event-filter').onchange=()=>{if(state)renderHistory();};

  return {
    render, showView,
    showMulDiv(open=true){unit='muldiv';$('follow-unit').checked=true;showView('alu',open);},
    inspectCache(level,address){$('cache-select').value=String(level);selectedLine=Math.floor(address/64)%state.caches[level].lines.length;$('follow-cache').checked=false;showView('cache',true);},
    inspectRam(address){memoryAddress=(address&~63)>>>0;$('follow-memory').checked=false;showView('ram',true);},
    followMemoryStage(access,open=true){
      if(!access)return;
      const last=access.steps.at(-1);
      const level=last?.cycle===state.cycle?(last.level??access.stage_level):(access.stage_level??last?.level);
      if(level!=null)$('cache-select').value=String(level);
      $('follow-cache').checked=true;showView('cache',open);
    },
    reset(address=0x80000000){muldivInspector.reset();memoryAddress=address>>>0;previousData=null;selectedLine=0;selectedRegister=10;selectedBit=0;traceKey='';registerKey='';renderedAluKey='';reveal=32;animating=false;unit='adder';$('follow-unit').checked=true;$('follow-register').checked=true;$('follow-cache').checked=true;showView('alu');},
    animate(time){if(animating && time-lastReveal>=110){lastReveal=time;reveal=Math.min(32,reveal+1);selectedBit=reveal-1;if(reveal===32)animating=false;renderAdder(state.datapath.alu);}},
  };
}
