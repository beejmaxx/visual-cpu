import {$,hex,escape,names} from './inspectors.js';
import {createViewport} from './viewport.js';
import {createCycleView} from './cycle-view.js';

const word=bytes=>bytes.reduce((v,b,i)=>(v|(b<<(i*8)))>>>0,0);
const overlaps=(a,size,b,width)=>a<b+width&&b<a+size;
const svgText=(x,y,text,cls='wave-text')=>`<text x="${x}" y="${y}" class="${cls}">${escape(text)}</text>`;

export function createExperience({seek,advance,watchEvent,inspectCache,inspectRam,pause,inspectCycle,finishWait}) {
  const viewport=createViewport();
  const cycleView=createCycleView({pause,inspect:inspectCycle,finishWait});
  let state,sim,address=0x80000000,size=4,array=null,selection={kind:'memory',target:address},selectedNode=null;
  let graph={nodes:[],descendants:[],registers:[]},lastOutput='',waveKey='',graphKey='';
  $('wave-register').innerHTML=names.map((name,i)=>`<option value="${i}" ${i===10?'selected':''}>x${i} / ${name}</option>`).join('');

  function setWatch(next,nextSize=size){
    address=next>>>0;size=Number(nextSize);selection={kind:'memory',target:address};selectedNode=null;
    $('watch-address').value=hex(address);$('watch-size').value=String(size);sim?.watch(address,size);if(state)render(state,sim);
  }
  function select(kind,target){selection={kind,target};selectedNode=null;if(kind==='register')$('wave-register').value=String(target);viewport.showTab('values');if(state)render(state,sim);}
  function jump(cycle){if(state&&cycle>=state.timeline_start&&cycle<=state.timeline_end)seek(cycle);}

  function waves(){
    if(!state||!sim)return;
    const register=Number($('wave-register').value),span=Number($('wave-span').value);
    const expanded=$('instrument-dock').classList.contains('expanded-scope');
    const key=`${state.cycle}/${state.timeline_start}/${state.timeline_end}/${register}/${span}/${expanded}`;
    if(key===waveKey)return;waveKey=key;
    const data=JSON.parse(sim.waves(register,span)),samples=data.samples;
    $('cycle-scrubber').min=state.timeline_start;$('cycle-scrubber').max=state.timeline_end;$('cycle-scrubber').value=state.cycle;
    $('cycle-target').min=state.timeline_start;$('cycle-target').max=state.timeline_end;$('cycle-target').value=state.cycle;
    $('history-range').textContent=`${state.timeline_start.toLocaleString()}–${state.timeline_end.toLocaleString()} retained`;
    $('cycle-back').disabled=state.cycle<=state.timeline_start;$('cycle-forward').disabled=state.halted&&state.cycle===state.timeline_end;
    $('to-frontier').disabled=state.cycle===state.timeline_end;
    const n=Math.max(samples.length,1),left=82,plot=expanded?1000:598,width=plot/n;
    const bottom=expanded?214:139;
    $('waveform').setAttribute('viewBox',`0 0 ${left+plot+8} ${bottom}`);
    let html='';
    const labels=expanded?[['CLK ↑',37],['Phase',64],[`x${register} Q`,91],[`x${register} D`,116],['WE',141],['ALU busy',164],['Mem req',187],['Mem ready',210]]:[['Phase',33],[`x${register} stored`,55],[`x${register} input`,77],['Reg write',97],['Mem req',116],['Mem ready',136]];
    for(const [label,y] of labels)html+=svgText(5,y,label,'wave-label');
    for(let k=0;k<n;k++){
      const x=left+k*width;html+=`<path class="wave-grid" d="M${x} 18 V${bottom}"/>`;
      if(samples[k]&&(k%Math.max(1,Math.ceil(n/12))===0))html+=svgText(x+2,12,samples[k].cycle,'wave-label');
    }
    const binary=(field,y,cls='wave-high')=>{
      let path='';for(let k=0;k<samples.length;k++){const x=left+k*width,hi=samples[k][field]?y-11:y;path+=`${k?'L':'M'}${x} ${hi} H${x+width}`;}
      return `<path class="${cls}" d="${path}"/>`;
    };
    const bus=(field,y,format)=>{
      let out='';for(let k=0;k<samples.length;){let end=k+1;while(end<samples.length&&samples[end][field]===samples[k][field])end++;
        const x=left+k*width,w=(end-k)*width,v=samples[k][field];
        if(v!=null){out+=`<rect class="wave-bus" x="${x+1}" y="${y}" width="${Math.max(0,w-2)}" height="18" rx="2"/>`;if(w>22)out+=svgText(x+5,y+12,w>70?format(v):field==='phase'?String(v).slice(0,1):'…');}k=end;
      }return out;
    };
    let clock='';for(let k=0;k<samples.length;k++){const x=left+k*width;clock+=`M${x} 40 V28 H${x+width*.5} V40 H${x+width}`;}
    if(expanded){html+=`<path class="wave-low" d="${clock}"/>`+bus('phase',51,v=>v)+bus('q',77,v=>hex(v))+bus('selected_d',102,v=>hex(v));html+=binary('we',143)+binary('alu_busy',166,'wave-d')+binary('memory_request',189)+binary('memory_ready',212);}
    else{html+=bus('phase',20,v=>v)+bus('q',42,v=>hex(v))+bus('selected_d',64,v=>hex(v));html+=binary('we',99)+binary('memory_request',118)+binary('memory_ready',137);}
    samples.forEach((s,k)=>{
      const x=left+k*width;if(s.cycle===state.cycle)html+=`<rect class="wave-cursor" x="${x}" y="18" width="${width}" height="${bottom-19}"/>`;
      html+=`<rect class="wave-hit" data-cycle="${s.cycle}" x="${x}" y="18" width="${width}" height="${bottom-19}"><title>Cycle ${s.cycle} · ${s.phase} · PC ${hex(s.pc)}\nQ=${hex(s.q)} · D=${s.selected_d==null?'—':hex(s.selected_d)} · WE=${Number(s.we)}${s.memory_stage?'\n'+escape(s.memory_stage):''}</title></rect>`;
    });
    if(!samples.length)html+=svgText(110,95,'Advance the CPU to record clock edges.','wave-label');
    $('waveform').innerHTML=html;
    const current=samples.find(s=>s.cycle===state.cycle);
    $('latch-register').textContent=`x${register} / ${names[register]}`;
    $('latch-d').textContent=current?.selected_d==null?'—':hex(current.selected_d);$('latch-q').textContent=hex(state.regs[register]);
    $('latch-clock').textContent=`↑ CLK · WE=${Number(Boolean(current?.we))}`;
    document.querySelector('.latch-summary').classList.toggle('writing',Boolean(current?.we));
  }

  function locations(){
    const ram=sim.memory(address,size),a=state.access;
    let html=`<button class="value-location resident" data-location="ram"><span>RAM</span><code>${ram.length===size?hex(word(ram),size*2):'unmapped'}</code><small>0x${hex(address)}</small></button>`;
    for(const level of [3,2,1]){
      const cache=state.caches[level],index=Math.floor(address/64)%cache.lines.length,line=cache.lines[index];
      const resident=line.valid&&line.address===(address&~63)>>>0&&address%64+size<=64;
      const pending=a&&!a.complete&&a.kind!=='fetch'&&overlaps(a.address,a.size,address,size)&&a.stage_level===level;
      html+=`<button class="value-location ${resident?'resident':''} ${pending?'pending':''}" data-location="${level}"><span>${cache.name}</span><code>${resident?hex(word(line.data.slice(address%64,address%64+size)),size*2):'not resident'}</code><small>set ${index} · byte ${address%64}${pending?' · '+escape(a.stage):''}</small></button>`;
    }
    const registers=graph.registers.filter(r=>r!==0);
    html+=`<div class="value-location ${registers.length?'resident':''}"><span>Dependent registers</span><code>${registers.length?registers.map(r=>`x${r}`).join(', '):'none yet'}</code><small>tracked writes at this cycle</small></div>`;
    $('value-locations').innerHTML=html;
    document.querySelectorAll('[data-register]').forEach(el=>el.classList.toggle('tracked',registers.includes(Number(el.dataset.register))));
    const d=state.datapath,related=id=>id!=null&&graph.descendants.includes(id);
    document.querySelector('.a-node').classList.toggle('tracked',related(d?.source_a));document.querySelector('.b-node').classList.toggle('tracked',related(d?.source_b));
    const dataAccess=a&&a.kind!=='fetch'&&overlaps(a.address,a.size,address,size);
    $('load-store-node').classList.toggle('tracked',Boolean(dataAccess&&!a.complete));
    $('muldiv-node').classList.toggle('tracked',Boolean(d?.alu?.unit==='muldiv'&&(related(d.source_a)||related(d.source_b))));
    for(const [id,on] of [['bus-fetch',a?.kind==='fetch'&&!a.complete],['bus-instruction',a?.kind==='fetch'&&a.complete&&a.finished_cycle===state.cycle],['bus-data',a&&a.kind!=='fetch'&&!a.path.includes('UART')&&!a.complete],['bus-output',a?.path.includes('UART')&&a.kind==='store']])$(id).classList.toggle('on',Boolean(on));
    $('watch-go').disabled=state.halted||ram.length!==size;
    $('watch-label').textContent=`${array&&address>=array.address&&address<array.address+array.size?`${array.name}[${Math.floor((address-array.address)/4)}] · `:''}0x${hex(address)} = ${ram.length===size?word(ram):'unmapped'}`;
    const effectiveAddress=d?.memory_address!=null&&overlaps(d.memory_address,size,address,size);
    $('watch-state').textContent=dataAccess&&!a.complete?a.stage:d?.alu?.unit==='muldiv'&&(related(d.source_a)||related(d.source_b))?`${d.instruction} · execute ${d.alu.muldiv.execute_cycle}/34`:effectiveAddress&&!d.committed?`Address ready · ${d.instruction} · 0x${hex(d.memory_address)}`:`${registers.length} dependent register${registers.length===1?'':'s'} · select a register or RAM byte to inspect its origin`;
    if(array){
      const bytes=sim.memory(array.address,Math.min(array.size,32));
      $('watch-array').innerHTML=Array.from({length:Math.floor(bytes.length/4)},(_,i)=>{const at=array.address+i*4;return `<button class="array-word ${at===address?'active':''}" data-word="${at}"><span>${escape(array.name)}[${i}]</span><strong>${word(bytes.slice(i*4,i*4+4))}</strong><small>0x${hex(at)}</small></button>`;}).join('');
    }
  }

  function nodeDetail(node){
    if(!node){$('trace-detail').textContent='';return;}
    const parents=node.parents.map(p=>{const parent=graph.nodes.find(n=>n.id===p.id);return `<button data-origin="${p.id}" title="${parent?escape(parent.label):'Older source not retained'}">${escape(p.role)} ← #${p.id}${parent?' · '+hex(parent.value):''}</button>`;}).join('');
    $('trace-detail').innerHTML=`<strong>#${node.id} · ${escape(node.label)} · ${hex(node.value)} (${node.value})</strong>Cycle ${node.cycle} · ${node.pc?'PC 0x'+hex(node.pc):'source value'}${node.address!=null?' · address 0x'+hex(node.address):''}${node.register!=null?' · x'+node.register+' / '+names[node.register]:''}<br>${parents}${node.cycle>=state.timeline_start&&node.cycle<=state.timeline_end?`<button data-jump="${node.cycle}">Seek cycle ${node.cycle} →</button>`:'<span> · cycle outside retained history</span>'}${node.access?`<div>${node.access.path.map(escape).join(' → ')} · ${node.access.latency} cycles</div>`:''}`;
  }

  function origins(){
    graph=JSON.parse(sim.trace(selection.kind,selection.target,size));
    $('trace-selection').textContent=selection.kind==='register'?`x${selection.target} / ${names[selection.target]} · ${hex(state.regs[selection.target])}`:selection.kind==='output'?`UART output byte ${selection.target}`:`RAM 0x${hex(selection.target)} · ${size} B`;
    const key=JSON.stringify([selection,graph,selectedNode]);
    if(key!==graphKey){
      graphKey=key;
      const dataNodes=new Set(graph.descendants),pending=[...graph.roots];
      while(pending.length){const node=graph.nodes.find(n=>n.id===pending.pop());for(const p of node?.parents||[]){if(p.role!=='address'&&!dataNodes.has(p.id)){dataNodes.add(p.id);pending.push(p.id);}}}
      const all=graph.nodes.filter(n=>dataNodes.has(n.id));
      const nodes=all.length>24?[...all.slice(0,1),...all.slice(-23)]:all;
      const positions=new Map(),depths=new Map(),rows=new Map();
      for(const node of nodes){const parents=node.parents.filter(p=>p.role!=='address'&&depths.has(p.id));const depth=parents.length?Math.max(...parents.map(p=>depths.get(p.id)))+1:0;depths.set(node.id,depth);const row=rows.get(depth)||0;rows.set(depth,row+1);positions.set(node.id,[12+depth*212,12+row*82]);}
      let html='<defs><marker id="value-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="5" markerHeight="5" orient="auto"><path d="M0 0L8 4L0 8Z" fill="#64838c"/></marker></defs>';
      for(const node of nodes){const [x,y]=positions.get(node.id);for(const p of node.parents){if(p.role==='address'||!positions.has(p.id))continue;const [px,py]=positions.get(p.id);html+=`<path class="value-edge" d="M${px+186} ${py+32} C${px+203} ${py+32},${x-18} ${y+32},${x} ${y+32}" marker-end="url(#value-arrow)"/>`;}}
      for(const node of nodes){const [x,y]=positions.get(node.id),root=graph.roots.includes(node.id);const label=node.kind==='initial_memory'?'RAM · initial bytes':node.kind==='initial_register'?node.label:node.kind==='output'?'UART write':node.label;
        html+=`<g class="value-node ${root?'root':''} ${selectedNode===node.id?'selected':''}" data-origin="${node.id}" role="button" tabindex="0" aria-label="${escape(label)} at cycle ${node.cycle}"><rect x="${x}" y="${y}" width="186" height="65" rx="5"/>${svgText(x+10,y+16,`#${node.id} · ${node.kind} · @${node.cycle}`,'origin-label')}${svgText(x+10,y+35,label.length>25?label.slice(0,24)+'…':label,'')}${svgText(x+10,y+54,hex(node.value)+'  '+node.value,'origin-value')}</g>`;
      }
      const width=Math.max(960,(Math.max(0,...depths.values())+1)*212+10),height=Math.max(96,Math.max(1,...rows.values())*82+12);
      const svg=$('value-graph');svg.setAttribute('viewBox',`0 0 ${width} ${height}`);svg.style.width=`${width}px`;svg.style.height=`${height}px`;
      if(!nodes.length)html+=svgText(14,40,selection.kind==='memory'?'No observed read or write of these bytes yet. Use Next value event.':'No recorded producer at this cycle.','wave-label');
      svg.innerHTML=html;
      $('trace-note').textContent=`Links follow data dependencies between writes; pointer/address inputs are listed on selection.${all.length>24?' Showing the source and latest 23 dependent writes.':''}${graph.missing?' Some older producers are no longer retained.':''}${graph.memory_limit_reached?' Memory provenance limit reached; some bytes have unknown earlier sources.':''}`;
    }
    nodeDetail(graph.nodes.find(n=>n.id===selectedNode));
  }

  function output(){
    if(lastOutput===state.console)return;lastOutput=state.console;
    const bytes=sim.output_bytes();
    $('output-bytes').innerHTML=Array.from(bytes.slice(-128),(b,i)=>`<button data-output="${Math.max(0,bytes.length-128)+i}" title="Trace output byte ${Math.max(0,bytes.length-128)+i}">${hex(b,2)}</button>`).join('');
  }

  function render(next,nextSim){state=next;sim=nextSim;waves();origins();locations();output();cycleView.render(state);}
  window.addEventListener('scope-layout',()=>waves());
  $('watch-go').onclick=()=>watchEvent();
  $('watch-array').onclick=e=>{const el=e.target.closest('[data-word]');if(el)setWatch(Number(el.dataset.word),4);};
  $('watch-address').onchange=e=>{const value=e.target.value.replace(/^0x/,'');if(/^[\da-f]{1,8}$/i.test(value)){e.target.setCustomValidity('');setWatch(parseInt(value,16));}else{e.target.setCustomValidity('Enter up to eight hexadecimal digits.');e.target.reportValidity();}};
  $('watch-size').onchange=e=>setWatch(address,Number(e.target.value));
  $('wave-register').onchange=$('wave-span').onchange=()=>waves();
  $('cycle-scrubber').oninput=e=>jump(Number(e.target.value));
  $('cycle-target').onchange=e=>{const cycle=Number(e.target.value);if(Number.isInteger(cycle))jump(cycle);e.target.value=state.cycle;};
  $('cycle-back').onclick=()=>state&&jump(state.cycle-1);$('cycle-forward').onclick=()=>state&&(state.cycle<state.timeline_end?jump(state.cycle+1):advance());
  $('to-frontier').onclick=()=>state&&jump(state.timeline_end);
  $('waveform').onclick=e=>{const el=e.target.closest('[data-cycle]');if(el)jump(Number(el.dataset.cycle));};
  $('trace-memory').onclick=()=>select('memory',address);
  $('value-locations').onclick=e=>{const el=e.target.closest('[data-location]');if(!el)return;el.dataset.location==='ram'?inspectRam(address):inspectCache(Number(el.dataset.location),address);};
  function originClick(e){const jumpTo=e.target.closest('[data-jump]');if(jumpTo){jump(Number(jumpTo.dataset.jump));return;}const el=e.target.closest('[data-origin]');if(el){selectedNode=Number(el.dataset.origin);const node=graph.nodes.find(n=>n.id===selectedNode);if(node){nodeDetail(node);if(e.currentTarget===$('value-graph'))jump(node.cycle);}if(state)render(state,sim);}}
  $('value-graph').onclick=$('trace-detail').onclick=originClick;
  $('value-graph').onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();originClick(e);}};
  $('output-bytes').onclick=e=>{const el=e.target.closest('[data-output]');if(el)select('output',Number(el.dataset.output));};
  return {
    render,viewport,replayClock:pauseMachine=>cycleView.replay(pauseMachine),showClockEdge:()=>cycleView.edge(),animate:time=>cycleView.animate(time),
    selectRegister(register){select('register',register);},
    selectMemory(at){setWatch(at,1);viewport.showTab('values');},
    reset(next,nextSim){state=next;sim=nextSim;lastOutput='';waveKey='';graphKey='';$('output-bytes').innerHTML='';cycleView.reset();viewport.reset();viewport.showTab('signals');array=state.symbols.find(s=>s.name==='samples')||state.symbols.find(s=>['numbers','values'].includes(s.name)&&s.kind===1)||null;$('watch-array').innerHTML='';$('wave-register').value='10';setWatch(array?array.address+(array.name==='samples'?8:0):state.symbols.find(s=>s.kind===1)?.address??0x80000000,4);},
  };
}
