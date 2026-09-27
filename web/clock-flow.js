import {hex,names} from './inspectors.js';

const value=n=>n==null?'—':`${n} · 0x${hex(n)}`;
const card=(name,value,detail='')=>({name,value,detail});
const register=n=>`x${n} / ${names[n]}`;
export const clockWrites=state=>state.cycle_detail.phase==='Writeback'&&Boolean(state.datapath?.committed&&state.datapath.write_enable&&state.datapath.rd!==0&&state.last_written===state.datapath.rd);

// All values describe the selected, already-recorded clock. No display step
// executes instructions or invents work during a modeled memory delay.
export function clockFlow(state){
  const c=state.cycle_detail,p=state.datapath,a=p?.alu,m=c.memory_clock,access=state.access;
  const flow={cards:[],sources:[],work:[],destinations:[],inputWires:[],workWires:[],edgeWires:[],memoryWire:null,returning:false};
  if(!c.phase){flow.cards=[card('Program counter',value(state.pc),'Address of the first instruction'),card('First fetch','L1I lookup','Press Next clock'),card('Instruction register','Empty','No instruction has been fetched')];flow.sources=['pc'];return flow;}
  if(m&&access){
    const cacheIndex=state.caches.findIndex(cache=>cache.name===m.component);
    const probe=access.probes.find(probe=>probe.level===cacheIndex);
    const address=`0x${hex(access.address)} · ${access.size} B`;
    flow.sources=[c.phase==='Fetch'?'pc':'memory'];flow.work=[m.component];flow.destinations=[m.component];
    flow.inputWires=c.phase==='Fetch'&&m.component==='L1I'?['bus-fetch']:c.phase==='Memory'&&m.component==='L1D'?['bus-data']:[];
    flow.memoryWire=m.component==='L2'?(access.kind==='fetch'?'L2I':'L2D'):m.component==='L3'?'L3':m.component==='RAM'?'RAM':null;
    if(m.stage_kind==='fill'){flow.sources=[];flow.memoryWire=m.component==='L3'?'RAM':m.component==='L2'?'L3':access.kind==='fetch'?'L2I':'L2D';flow.returning=true;}
    if(m.remaining_after>0){
      flow.cards=[card(access.kind==='store'?'Store request held':'Read request held',address,access.kind==='store'?`Data ${value(access.value)}`:`Line 0x${hex(access.line)} · 64 B`),card(`${m.component} latency`,`${m.remaining_before} → ${m.remaining_after} clocks`,`${m.latency}-clock ${m.stage_kind==='lookup'?'lookup':'access'} model`),card('No data transfer','Ready = 0','PC, registers and data bytes hold')];
      flow.destinations=[];return flow;
    }
    if(m.stage_kind==='lookup'){
      flow.cards=[card('Request address',address,`Set ${probe?.index??'—'} · byte ${access.address%64}`),card(`${m.component} tag comparison`,probe?`V=${Number(probe.valid)} · ${probe.hit?'HIT':'MISS'}`:'Lookup completed',probe?`stored ${probe.stored_tag==null?'—':hex(probe.stored_tag)} / requested ${hex(probe.tag)}`:''),card(probe?.hit?'Read selected bytes':'Request next level',probe?.hit?`Line 0x${hex(access.line)}`:'No matching valid line',c.next)];
    }else if(m.stage_kind==='read'){
      flow.sources=['RAM'];
      flow.cards=[card('RAM line request',`0x${hex(access.line)} · 64 B`),card('RAM response','Latency 1 → 0','64 bytes become available'),card('Refill buffer','64 bytes received',c.next)];flow.returning=true;
    }else if(m.stage_kind==='fill'){
      const cache=state.caches[cacheIndex],index=Math.floor(access.address/64)%cache.lines.length;
      flow.cards=[card('Refill buffer',`0x${hex(access.line)} · 64 B`),card(`Write ${m.component} set ${index}`,'Tag + data + valid bit','One cache level fills this clock'),card(`${m.component} line installed`,`V = 1 · set ${index}`,c.next)];
    }else if(m.stage_kind==='write'){
      flow.cards=[card('Store data',value(access.value),address),card('Write-through completes','Latency 1 → 0','Update RAM and resident cache copies'),card('Memory bytes',c.changes[0]?.after??'Updated',`${c.changes[0]?.before??''} → ${c.changes[0]?.after??''}`)];flow.edgeWires=['wire-store'];
    }else{
      flow.cards=[card('Device address',address),card(access.kind==='store'?'UART transmit':'UART read',value(access.value)),card(access.kind==='store'?'Terminal output':'Load result',access.kind==='store'?`Byte 0x${hex(access.value&255,2)}`:value(access.value),c.next)];flow.edgeWires=access.kind==='store'?['bus-output']:['wire-load'];
    }
    if(access.complete&&access.kind==='fetch'){
      flow.cards[2]=card('Instruction register',`0x${hex(c.after.instruction_word)}`,state.instruction?.text??'Decode on the next clock');flow.destinations.push('instruction');flow.edgeWires.push('bus-instruction');
    }else if(access.complete&&access.kind==='load'){
      flow.cards[2]=card('Load-result latch',value(c.after.write_value),'Destination register waits for Writeback');flow.destinations.push('write');flow.edgeWires.push('wire-load','wire-write-data');
    }
    return flow;
  }
  if(c.phase==='Decode'&&p){
    flow.cards=[card('Instruction register',`0x${hex(p.raw)}`,p.instruction),card('Decode and select',`A ← ${p.left_source} · B ← ${p.right_source}`,`Operation: ${p.operation}`),card('Operand latches',`A = ${p.left} · B = ${p.right}`,'Capture the selected operands')];
    flow.sources=['instruction','registers'];flow.work=['decoder'];flow.destinations=['operand-a','operand-b'];
    flow.workWires=[p.left_source==='PC'?'wire-pc-a':p.read_a?'wire-read-a':null,p.right_source==='immediate'?'wire-immediate':p.read_b?'wire-read-b':null].filter(Boolean);flow.edgeWires=flow.workWires;return flow;
  }
  if(c.phase==='Execute'&&p){
    flow.sources=['operand-a','operand-b'];flow.work=[a?.muldiv?'muldiv':'alu'];flow.destinations=['write'];
    flow.workWires=a?.muldiv?['wire-muldiv-a','wire-muldiv-b']:['wire-alu-a','wire-alu-b'];
    flow.edgeWires=p.memory_address!=null?['wire-address']:a?.muldiv?(a.muldiv.result!=null?['wire-muldiv-result']:[]):p.write_value!=null?['wire-result','wire-write-data']:[];
    if(a?.muldiv){
      const md=a.muldiv;
      flow.cards=[card('Working registers',md.registers.map(r=>`${r.name.split(' · ')[0]}=${r.before}`).join(' · '),'Values before this iteration'),card(`${md.kind} · ${md.stage}`,md.stage==='iterate'?`Process bit ${Math.max(0,md.iteration-1)}`:md.stage==='prepare'?'Load operands':'Select signed result',md.description),card(md.result!=null?'Result ready':'Working registers latch',md.result!=null?value(md.result):md.registers.map(r=>`${r.name.split(' · ')[0]}=${r.after}`).join(' · '),'Architectural destination still holds its old value')];flow.destinations=md.result!=null?['muldiv','write']:['muldiv'];
    }else{
      flow.cards=[card('Latched operands',`A = ${p.left} · B = ${p.right}`,`${hex(p.left)} · ${hex(p.right)}`),card(a?.function?.toUpperCase()??p.operation,value(a?.result),p.branch_taken!=null?`Branch ${p.branch_taken?'taken':'not taken'}`:a?.unit==='adder'?'Full-adder sum and carry signals':a?.unit==='shifter'?'Selected barrel-shifter stages':a?.unit==='logic'?'Independent bitwise gates':a?.unit==='compare'?'Comparison predicate':'Control operation'),card(p.memory_address!=null?'Address latch':'Result latch',value(p.memory_address??p.write_value),p.rd!=null&&p.rd!==0?`${register(p.rd)} is not written yet`:c.next)];
      if(p.memory_address!=null)flow.destinations=['memory'];
      else if(p.branch_taken!=null){flow.destinations=['pc'];flow.edgeWires=[];flow.cards[2]=card('Pending next PC',`0x${hex(p.next_pc)}`,'PC commits at Writeback');}
      else if(p.write_value==null){flow.destinations=[];flow.cards[2]=card('Control state','No register result',c.next);}
      if(!a){flow.work=[];flow.workWires=[];}
    }return flow;
  }
  if(c.phase==='Writeback'){
    const rd=state.last_written,write=clockWrites(state);
    flow.cards=[card('Write-port input',write?value(c.after.regs[rd]):'No register data',write?`Destination ${register(rd)}`:'Write enable = 0'),card('Clock edge',write?'Write enable = 1':'Complete instruction','Commit state and advance PC'),card(write?register(rd):'Program counter',write?`${c.before.regs[rd]} → ${c.after.regs[rd]}`:`0x${hex(c.before.pc)} → 0x${hex(c.after.pc)}`,write?`PC ${hex(c.before.pc)} → ${hex(c.after.pc)}`:c.next)];
    flow.sources=['write'];flow.work=['write'];flow.destinations=write?['registers','pc']:['pc'];flow.workWires=write?['wire-write-data']:[];flow.edgeWires=write?['wire-writeback']:[];return flow;
  }
  flow.cards=[card('Pending result',value(p?.write_value)),card('Memory phase','No load or store','This core spends one clock passing through'),card('State held','No data transfer',c.next)];flow.work=['memory'];return flow;
}
