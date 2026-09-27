const $ = id => document.getElementById(id);
const text = (id,value) => { $(id).textContent=value; };
const escape = value => String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const bitAt = (word,bit) => Number((BigInt('0x'+word)>>BigInt(bit))&1n);
const hex = value => (value>>>0).toString(16).padStart(8,'0');

export function createMulDivInspector({nextCycle}) {
  let unit=null, circuitIndex=0, selectedBit=0, phaseKey='';
  // Both inspectors use the same full-adder drawing, with independent signals.
  const gate=$('gate-diagram').cloneNode(true);
  gate.id='md-gate-diagram';
  gate.querySelectorAll('[id]').forEach(el=>el.id='md-'+el.id);
  $('md-gates').append(gate);

  const label=(x,y,value,cls='note')=>`<text x="${x}" y="${y}" class="${cls.endsWith('-label')?'wire-label ':''}${cls}">${escape(value)}</text>`;
  const wire=(path,on=true,kind='read-a')=>`<path class="signal ${kind} ${on?'on':''}" d="${path}" marker-end="url(#arrow)"/>`;
  const box=(x,y,w,h,title,value,note='')=>`<g class="node"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="5"/>${label(x+13,y+22,title,'label')}${label(x+13,y+45,value,'small-value')}${note?label(x+13,y+h-10,note):''}</g>`;

  function diagram(m) {
    let html='';
    if(m.stage!=='iterate') {
      m.circuits.forEach((c,i)=>{
        const y=18+i*130;
        html+=box(20,y,225,86,c.name.toUpperCase(),`B = ${c.right}`,`SUB = ${Number(c.subtract)} · A = 0`);
        html+=wire(`M245 ${y+43} H325`,true,'read-b');
        html+=box(325,y,310,86,`${c.width}-BIT CONDITIONAL NEGATION`,`0 + (B XOR SUB) + SUB`,`${c.subtract?'two’s complement':'positive / unsigned pass'}`);
        html+=wire(`M635 ${y+43} H705`,true,'write');
        html+=box(705,y,270,86,'LATCHED OUTPUT',c.output,`carry out ${c.carry_out}`);
      });
      if(m.circuits.length===1)html+=label(20,152,`Selected word: ${m.result==null?'pending':hex(m.result)}`,'value');
    } else {
      const registers=m.kind==='divide'?[m.registers[0],m.registers[2],m.registers[1]]:m.registers;
      registers.forEach((r,i)=>{
        html+=box(20+i*330,12,300,86,r.name.toUpperCase(),`${r.before} →`,`${r.after} · ${r.width} bits after this cycle`);
      });
      const c=m.circuits[0], control=m.control;
      if(m.kind==='multiply') {
        html+=wire('M500 98 V132',true,'read-b');
        html+=wire('M830 98 V118 H635 V168 H615',true,'control');
        html+=label(670,137,`Q[0] = ${control.select}`,'control-label');
        html+=box(350,133,265,66,'64 PARALLEL AND GATES',control.output);
        html+=wire('M170 98 V264 H350');
        html+=wire('M485 199 V232',true,'read-b');
        html+=box(350,233,265,66,'64-BIT RIPPLE-CARRY ADDER',c.output);
        html+=wire('M615 266 H980 V330 H8 V55 H20',true,'write');
        html+=label(649,254,'P ← sum','write-label');
        html+=label(350,318,'M shifts left 1 · Q shifts right 1');
      } else {
        html+=wire('M170 98 V132');
        html+=wire('M500 98 V117 H300 V160 H285',true,'read-b');
        html+=box(20,133,265,66,'SHIFT R ← (R << 1) | Q[31]',c.left);
        html+=wire('M285 166 H365');
        html+=wire('M830 98 V164 H630',true,'read-b');
        html+=box(365,133,265,66,'33-BIT TRIAL SUBTRACTOR',c.output);
        html+=wire('M500 199 V233',control.select===1,'write');
        html+=wire('M300 166 V264 H365',control.select===0);
        html+=box(365,234,265,66,`REMAINDER MUX · C33 = ${control.select}`,control.output);
        html+=wire('M630 267 H980 V330 H8 V55 H20',true,'write');
        html+=label(650,222,control.select?'keep trial · Q[0] ← 1':'restore R · Q[0] ← 0','write-label');
        html+=label(20,284,'Q shifts left 1');
      }
    }
    $('md-diagram').innerHTML=html;
  }

  function renderCircuit() {
    if(!unit)return;
    const c=unit.circuits[circuitIndex], control=unit.control;
    selectedBit=Math.min(selectedBit,c.width-1);
    const indices=Array.from({length:c.width},(_,i)=>c.width-i-1);
    const rows=[];
    if(control?.kind==='and')rows.push(['M before',b=>bitAt(control.input_one,b)],['Q[0]',()=>control.select],['M AND Q[0]',b=>bitAt(control.output,b)]);
    rows.push(['A',b=>c.bits[b].a],['B',b=>c.bits[b].b],[c.subtract?'B′ = NOT B':'B′ = B',b=>c.bits[b].effective_b],['Carry in',b=>c.bits[b].carry_in,'carry'],['A XOR B′',b=>c.bits[b].propagate],['A AND B′',b=>c.bits[b].generate],['Sum',b=>c.bits[b].sum,'sum'],['Carry out',b=>c.bits[b].carry_out,'carry']);
    if(control?.kind==='mux')rows.push(['Restore R',b=>bitAt(control.input_zero,b)],['C33',()=>control.select],['Selected R',b=>bitAt(control.output,b),'sum']);
    $('md-bits').style.gridTemplateColumns=`96px repeat(${c.width},minmax(18px,1fr))`;
    $('md-bits').style.minWidth=`${Math.max(770,100+c.width*22)}px`;
    $('md-bits').innerHTML='<span class="row-label">bit</span>'+indices.map(b=>`<button class="bit-head ${b===selectedBit?'chosen':''}" data-md-bit="${b}" aria-label="Inspect multiply/divide bit ${b}">${b}</button>`).join('')+rows.map(([name,value,cls=''])=>`<span class="row-label">${name}</span>`+indices.map(b=>`<span class="bit-cell ${value(b)?'one':''} ${cls} ${b===selectedBit?'chosen':''} ${b%8===7?'byte-start':''}" data-md-bit="${b}" title="${name}[${b}] = ${value(b)}">${value(b)}</span>`).join('')).join('');
    text('md-circuit-summary',`${c.width} full adders · SUB=${Number(c.subtract)} · ${c.left} ${c.subtract?'−':'+'} ${c.right} = ${c.output} · C${c.width}=${c.carry_out}`);
    const b=c.bits[selectedBit];
    text('md-bit-title',`${c.name} · bit ${selectedBit}`);
    text('md-bit-equation',`${b.a} + ${b.effective_b} + Cin(${b.carry_in}) = ${b.sum} + 2 × Cout(${b.carry_out})`);
    for(const [id,value] of Object.entries({'gate-a':`A=${b.a}`,'gate-b':`B′=${b.effective_b}`,'gate-cin':`Cin=${b.carry_in}`,'gate-p':b.propagate,'gate-g':b.generate,'gate-h':b.propagate&b.carry_in,'gate-sum':b.sum,'gate-cout':b.carry_out,'gate-sum-label':`SUM=${b.sum}`,'gate-carry-label':`Cout=${b.carry_out}`}))text('md-'+id,value);
    gate.querySelectorAll('[data-gate-signal]').forEach(el=>el.classList.toggle('one',Boolean(el.dataset.gateSignal==='carry_term'?(b.propagate&b.carry_in):b[el.dataset.gateSignal])));
    $('md-bit-prev').disabled=selectedBit===0;$('md-bit-next').disabled=selectedBit===c.width-1;
    $('md-control-gates').hidden=!control;
    if(control)renderControlGate(control);
  }

  function renderControlGate(control) {
    const one=bitAt(control.input_one,selectedBit), zero=bitAt(control.input_zero,selectedBit), s=control.select;
    let html='';
    if(control.kind==='and') {
      html+=label(16,37,`M[${selectedBit}]=${one}`,'gate-input')+label(16,89,`Q[0]=${s}`,'gate-input');
      html+=`<path class="gate-wire ${one?'one':''}" d="M155 33 H270"/><path class="gate-wire ${s?'one':''}" d="M155 85 H225 V63 H270"/>`;
      html+=`<g class="gate"><rect x="270" y="18" width="105" height="62" rx="5"/><text x="322" y="43">AND</text><text class="gate-value" x="322" y="64">${one&s}</text></g>`;
      html+=`<path class="gate-wire ${one&s?'one':''}" d="M375 50 H470"/>`+label(485,55,`partial[${selectedBit}]=${one&s} → adder B[${selectedBit}]`,'gate-output');
    } else {
      const h=one&s, l=zero&(1-s);
      html+=label(10,25,`trial[${selectedBit}]=${one}`,'gate-input')+label(10,103,`restore[${selectedBit}]=${zero}`,'gate-input');
      html+=`<path class="gate-wire ${one?'one':''}" d="M175 20 H340"/><path class="gate-wire ${zero?'one':''}" d="M175 100 H340"/>`;
      html+=label(10,62,`C33=${s}`,'gate-input')+`<path class="gate-wire ${s?'one':''}" d="M99 58 H140 V41 H340 M115 58 V121 H205"/>`;
      html+=`<g class="gate"><rect x="205" y="110" width="75" height="30" rx="5"/><text x="242" y="130">NOT ${1-s}</text></g><path class="gate-wire ${1-s?'one':''}" d="M280 125 H340"/>`;
      html+=`<g class="gate"><rect x="340" y="7" width="85" height="49" rx="5"/><text x="382" y="27">AND</text><text class="gate-value" x="382" y="45">${h}</text></g><g class="gate"><rect x="340" y="87" width="85" height="49" rx="5"/><text x="382" y="107">AND</text><text class="gate-value" x="382" y="125">${l}</text></g>`;
      html+=`<path class="gate-wire ${h?'one':''}" d="M425 31 H505 V59 H555"/><path class="gate-wire ${l?'one':''}" d="M425 111 H525 V83 H555"/><g class="gate"><rect x="555" y="46" width="85" height="49" rx="5"/><text x="597" y="66">OR</text><text class="gate-value" x="597" y="84">${h|l}</text></g><path class="gate-wire ${h|l?'one':''}" d="M640 71 H690"/>`+label(702,75,`R[${selectedBit}]=${h|l}`,'gate-output');
    }
    $('md-control-gates').innerHTML=html;
  }

  function render(m) {
    unit=m;
    $('md-content').hidden=!m;$('md-empty').hidden=Boolean(m);
    text('md-progress',m?`Execute cycle ${m.execute_cycle} / 34`:'No multiply/divide operation');
    if(!m)return;
    const key=`${m.operation}/${m.stage}`;
    if(key!==phaseKey){phaseKey=key;circuitIndex=0;}
    circuitIndex=Math.min(circuitIndex,m.circuits.length-1);
    text('md-action',m.description);
    document.querySelectorAll('[data-md-phase]').forEach(el=>el.classList.toggle('active',el.dataset.mdPhase===m.stage));
    const current=m.kind==='multiply'?m.iteration-1:32-m.iteration;
    $('md-iterations').innerHTML=Array.from({length:32},(_,i)=>31-i).map(bit=>`<span class="${(m.kind==='multiply'?bit<m.iteration:bit>=32-m.iteration)?'done':''} ${m.stage==='iterate'&&bit===current?'current':''}" title="Operand bit ${bit}">${bit}</span>`).join('');
    const entries=[['A interpreted as',m.signed_a?'signed':'unsigned'],['B interpreted as',m.signed_b?'signed':'unsigned'],['Sign controls',`A− ${Number(m.negative_a)} · B− ${Number(m.negative_b)}`]];
    if(m.kind==='multiply')entries.push(['High 32 bits',m.high==null?'pending':hex(m.high)],['Low 32 bits',m.low==null?'pending':hex(m.low)]);
    else entries.push(['Quotient',m.quotient==null?'pending':hex(m.quotient)],['Remainder',m.remainder==null?'pending':hex(m.remainder)]);
    entries.push(['Selected result',m.result==null?'pending':hex(m.result)]);
    $('muldiv-signals').innerHTML=entries.map(([name,value])=>`<div class="signal-card">${name}<strong>${value}</strong></div>`).join('');
    text('md-special',m.divide_by_zero?'Divisor is zero: each trial subtract succeeds; quotient becomes all ones and remainder keeps the dividend.':m.signed_overflow?'Signed overflow: minimum signed integer divided by −1. Quotient is 80000000; remainder is zero.':m.kind==='divide'?`Quotient sign = A sign XOR B sign; remainder sign = A sign. R uses 33 bits for the shifted trial subtraction.`:'32 iterations form the 64-bit magnitude product; conditional negation applies its sign before selecting high or low bits.');
    $('md-circuit').innerHTML=m.circuits.map((c,i)=>`<option value="${i}">${escape(c.name)}</option>`).join('');$('md-circuit').value=String(circuitIndex);
    diagram(m);renderCircuit();
  }

  $('md-next').onclick=nextCycle;
  $('md-circuit').onchange=e=>{circuitIndex=Number(e.target.value);renderCircuit();};
  $('md-bits').onclick=e=>{const el=e.target.closest('[data-md-bit]');if(el){selectedBit=Number(el.dataset.mdBit);renderCircuit();}};
  $('md-bit-prev').onclick=()=>{selectedBit=Math.max(0,selectedBit-1);renderCircuit();};
  $('md-bit-next').onclick=()=>{selectedBit=Math.min(unit.circuits[circuitIndex].width-1,selectedBit+1);renderCircuit();};
  return {render,reset(){unit=null;phaseKey='';selectedBit=0;circuitIndex=0;}};
}
