import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';

// The production frontend and generated WASM run together; CPU state is not mocked.
test('WASM interface: datapath, gates, sequential cache stages, controls and programs', async () => {
  const base=new URL('../web/',import.meta.url);
  const dom=new JSDOM(await readFile(new URL('index.html',base),'utf8'),{url:'http://localhost:8765/'});
  globalThis.window=dom.window;globalThis.document=dom.window.document;
  const style=document.createElement('style');style.textContent=await readFile(new URL('datapath.css',base),'utf8')+await readFile(new URL('experience.css',base),'utf8')+await readFile(new URL('workspace.css',base),'utf8');document.head.append(style);
  let nextFrame;
  globalThis.requestAnimationFrame=callback=>{nextFrame=callback;};
  globalThis.fetch=async request=>{
    const url=request instanceof URL?request:new URL(request,base);
    try{return new Response(await readFile(fileURLToPath(url)),{headers:{'Content-Type':url.pathname.endsWith('.wasm')?'application/wasm':'text/plain'}});}
    catch{return new Response('',{status:404});}
  };
  await import('../web/main.js');
  const $=id=>document.getElementById(id);
  const click=selector=>document.querySelector(selector).dispatchEvent(new dom.window.MouseEvent('click',{bubbles:true}));
  const change=id=>$(id).dispatchEvent(new dom.window.Event('change',{bubbles:true}));
  let time=performance.now();
  function advance(frames=100){time=Math.max(time,performance.now());for(let n=0;n<frames;n++){time+=100;nextFrame(time);}}
  async function until(predicate){
    const deadline=Date.now()+5000;
    while(!predicate()){assert.ok(Date.now()<deadline,'async UI update timed out');await new Promise(r=>setTimeout(r,5));}
  }
  async function example(name){$('example').value=name;change('example');await until(()=>$('filename').textContent===`${name}.elf`);}
  async function exportSvg(id,name){
    if(!process.env.SIMULATOR_EXPORT_DIR)return;
    const directory=process.env.SIMULATOR_EXPORT_DIR;await mkdir(directory,{recursive:true});
    const svg=$(id).cloneNode(true), ns='http://www.w3.org/2000/svg';
    const vb=svg.getAttribute('viewBox').split(' ').map(Number);svg.setAttribute('width',1600);svg.setAttribute('height',Math.round(1600*vb[3]/vb[2]));
    const variables=Object.fromEntries([...style.textContent.matchAll(/(--[\w-]+):([^;}]+)/g)].map(m=>[m[1],m[2]]));
    const css=document.createElementNS(ns,'style');css.textContent=style.textContent.replace(/var\((--[\w-]+)\)/g,(_,key)=>variables[key]);svg.prepend(css);
    if(!svg.querySelector('defs'))svg.prepend($('diagram').querySelector('defs').cloneNode(true));
    // librsvg has no HTML foreignObject renderer. Use the same cell geometry and
    // text to inspect the register bank alongside the actual SVG wires/nodes.
    const foreign=svg.querySelector('foreignObject');
    if(foreign){
      const group=document.createElementNS(ns,'g');
      foreign.querySelectorAll('[data-register]').forEach((button,i)=>{
        const x=38+(i%2)*136.5,y=146+Math.floor(i/2)*19;
        const rect=document.createElementNS(ns,'rect');rect.setAttribute('x',x);rect.setAttribute('y',y);rect.setAttribute('width',128.5);rect.setAttribute('height',18);
        rect.setAttribute('fill',button.classList.contains('changed')?'#473f2b':button.classList.contains('read-a')?'#1a3535':button.classList.contains('read-b')?'#24364b':'#161e28');group.append(rect);
        for(const [value,tx,anchor,color] of [[button.firstElementChild.textContent,x+4,'start','#a8bbcf'],[button.lastElementChild.textContent,x+124,'end','#d4e0ec']]){
          const text=document.createElementNS(ns,'text');text.setAttribute('x',tx);text.setAttribute('y',y+12);text.setAttribute('text-anchor',anchor);text.setAttribute('fill',color);text.setAttribute('font-family','Menlo,monospace');text.setAttribute('font-size',9);text.textContent=value;group.append(text);
        }
      });foreign.replaceWith(group);
    }
    await writeFile(`${directory}/${name}.svg`,new dom.window.XMLSerializer().serializeToString(svg));
  }

  assert.equal($('filename').textContent,'flow.elf');
  // A stalled fetch still has an explainable clock, without pretending a
  // modeled RAM delay contains gate-level work. Display steps never tick it.
  for(let n=0;n<26;n++)$('cycle').click();
  assert.equal($('cycles').textContent,'26');assert.equal($('retired').textContent,'0');
  assert.match($('cycle-title').textContent,/RAM read: 59 clocks remaining/);
  assert.equal($('cycle-progress-count').textContent,'1 / 60 clocks');
  assert.equal(document.querySelector('[data-phase].active').dataset.phase,'Fetch');
  const heldClock=$('cycles').textContent;
  for(const step of [0,1,2]){click(`[data-subcycle="${step}"]`);assert.equal($('cycles').textContent,heldClock);assert.equal(document.querySelector('.inside-card.active').dataset.insideCard,String(step));}
  assert.match($('cycle-body').textContent,/no bytes return yet/);
  assert.match($('cycle-body').textContent,/No data value latched/);
  $('expand-cycle').click();assert.equal($('expand-cycle').getAttribute('aria-expanded'),'true');
  $('expand-scope').click();assert.equal($('expand-cycle').getAttribute('aria-expanded'),'false');
  assert.equal($('waveform').getAttribute('viewBox').split(' ')[3],'214');
  $('expand-scope').click();assert.equal($('waveform').getAttribute('viewBox').split(' ')[3],'139');
  $('toggle-code').click();assert.ok(document.body.classList.contains('hide-code'));$('toggle-code').click();
  $('fit-layout').click();assert.ok(!document.body.classList.contains('fit-workspace'));$('fit-layout').click();
  $('speed').value='4';$('finish-wait').click();advance(5);
  assert.equal($('cycles').textContent,'85');assert.equal($('cycle-title').textContent,'RAM returned a 64-byte line');
  $('step').click();assert.equal($('cycles').textContent,'92');assert.equal($('retired').textContent,'1');
  assert.equal(document.querySelector('[data-phase].active').dataset.phase,'Writeback');
  assert.match($('phase-caption').textContent,/next: Fetch/);
  $('cycle-target').value='26';change('cycle-target');assert.match($('cycle-title').textContent,/59 clocks remaining/);
  $('to-frontier').click();assert.equal($('cycles').textContent,'92');
  // A cache fill previews its real old/new valid count, and never marks the
  // newly queued L2 stage as the work that happened on this clock.
  $('cycle-target').value='86';change('cycle-target');
  click('[data-clock-step="0"]');assert.match($('cache-label-3').textContent,/^0 \/ 128/);
  click('[data-clock-step="1"]');assert.ok(document.querySelector('[data-cache="3"]').classList.contains('clock-work'));
  assert.ok(!document.querySelector('[data-cache="2"]').classList.contains('active'));
  click('[data-clock-step="2"]');assert.match($('cache-label-3').textContent,/^1 \/ 128/);
  assert.equal($('cycles').textContent,'86');
  $('to-frontier').click();
  $('speed').value='2';
  await example('alu');
  assert.equal($('error').hidden,true,$('error').textContent);
  assert.equal($('status').textContent,'Paused');
  assert.equal(document.querySelectorAll('#diagram [data-register]').length,32);
  assert.ok(document.querySelectorAll('.asm-line').length>30);
  $('cycle').click();assert.equal($('cycles').textContent,'1');
  $('back').click();assert.equal($('cycles').textContent,'0');
  $('to-main').click();advance(20);assert.equal($('symbol-label').textContent,'main');
  $('step').click();assert.equal(document.querySelector('[data-register="11"]').lastElementChild.textContent,'00000005');
  $('step').click();assert.equal(document.querySelector('[data-register="12"]').lastElementChild.textContent,'00000003');
  $('stop-event').value='8';$('next-event').click();advance(5);
  assert.equal($('operand-a').textContent,'00000005');assert.equal($('operand-b').textContent,'00000003');
  assert.equal($('alu-output').textContent,'00000008');
  click('[data-clock-step="0"]');assert.equal($('execute-detail').textContent,'—');
  click('[data-clock-step="1"]');assert.equal($('execute-detail').textContent,'00000008');
  assert.ok($('alu-node').classList.contains('clock-work'));
  click('[data-clock-step="2"]');
  assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000000','result must not write before writeback');
  assert.match($('write-enable').textContent,/WE = 0/);
  click('#decoder-node');assert.match($('decode-format').textContent,/R format/);assert.match($('decode-signals').textContent,/x11 = 00000005/);
  click('[data-inspector="alu"]');
  click('#adder-bits [data-bit="3"]');assert.equal($('gate-cin').textContent,'Cin=1');assert.equal($('gate-sum-label').textContent,'SUM=1');
  const frozenCycle=$('cycles').textContent;
  click('[data-subcycle="1"]');$('cycle-open-logic').click();
  assert.equal($('cycles').textContent,frozenCycle);assert.ok(document.querySelector('.machine-workspace.inspecting'));
  click('.expand-inspection');click('.close-inspection');
  assert.ok(!document.querySelector('.wide-inspector'));assert.ok(!document.querySelector('.machine-workspace.inspecting'));
  $('cycle-inspect').click();
  $('propagate').click();assert.ok(document.querySelectorAll('.bit-cell.unknown').length>0);advance(80);
  assert.equal($('cycles').textContent,frozenCycle,'carry display must not advance CPU time');
  assert.equal(document.querySelectorAll('#adder-bits .unknown').length,0);
  click('#adder-bits [data-bit="3"]');
  await exportSvg('diagram','add-before-writeback');await exportSvg('gate-diagram','adder-bit-3');
  $('step').click();assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000008');
  assert.match($('write-enable').textContent,/WE = 1/);
  assert.ok(document.querySelector('[data-register="10"]').classList.contains('changed'));
  const writeClock=$('cycles').textContent;
  click('[data-clock-step="0"]');assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000000');
  $('clock-next-signal').click();assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000000');assert.match($('write-enable').textContent,/WE = 1.*pending/);
  $('clock-next-signal').click();assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000008');
  assert.equal($('cycles').textContent,writeClock);
  $('clock-replay').click();assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000000');
  advance(30);assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000008');assert.equal($('cycles').textContent,writeClock);
  $('back').click();assert.equal(document.querySelector('[data-register="10"]').lastElementChild.textContent,'00000000');
  $('step').click();$('step').click();assert.match($('adder-summary').textContent,/NOT\(B\) \+ 1/);
  $('step').click();assert.equal($('logic-view').hidden,false);assert.equal($('alu-output').textContent,'0000000d');
  $('step').click();assert.equal($('shifter-view').hidden,false);assert.equal($('shift-stages').children.length,5);assert.equal($('alu-output').textContent,'00000068');
  $('step').click();assert.equal($('compare-view').hidden,false);assert.equal($('alu-output').textContent,'00000001');
  $('stop-event').value='64';$('next-event').click();advance(5);
  assert.equal($('muldiv-view').hidden,false);assert.match($('md-progress').textContent,/1 \/ 34/);
  assert.equal($('alu-output').textContent,'pending');
  assert.equal(document.querySelector('[data-register="29"]').lastElementChild.textContent,'00000000');
  $('md-next').click();advance(5);
  assert.match($('md-progress').textContent,/2 \/ 34/);
  assert.equal(document.querySelectorAll('#md-bits .bit-head').length,64);
  assert.match($('md-circuit-summary').textContent,/0000000000000000 \+ 0000000000000005 = 0000000000000005/);
  click('#md-bits [data-md-bit="0"]');assert.equal($('md-gate-sum-label').textContent,'SUM=1');
  assert.match($('md-control-gates').textContent,/partial\[0\]=1/);
  await exportSvg('md-diagram','multiply-bit-0');await exportSvg('md-control-gates','multiply-and-bit-0');
  $('md-next').click();advance(5);
  assert.match($('md-circuit-summary').textContent,/0000000000000005 \+ 000000000000000a = 000000000000000f/);
  assert.equal($('alu-output').textContent,'pending','partial product is not the completed ALU result');
  $('stop-event').value='8';$('next-event').click();advance(5);
  assert.match($('md-progress').textContent,/34 \/ 34/);assert.match($('muldiv-signals').textContent,/0000000f/);
  assert.equal(document.querySelector('[data-register="29"]').lastElementChild.textContent,'00000000');
  $('step').click();assert.equal(document.querySelector('[data-register="29"]').lastElementChild.textContent,'0000000f');

  // Signed -17 / 5: first restore after a borrowed subtraction, later keep it.
  $('md-next').click();advance(5);assert.match($('md-progress').textContent,/1 \/ 34/);
  assert.match($('alu-detail').textContent,/div t0/);
  assert.match($('md-circuit-summary').textContent,/ffffffef = 00000011/);
  $('md-next').click();advance(5);
  assert.equal(document.querySelectorAll('#md-bits .bit-head').length,33);
  assert.match($('md-action').textContent,/reject subtraction/);
  assert.match($('md-control-gates').textContent,/C33=0/);
  for(let iteration=2;iteration<=31;iteration++){$('md-next').click();advance(2);}
  assert.match($('md-action').textContent,/keep subtraction/);
  assert.match($('md-control-gates').textContent,/C33=1/);
  click('#md-bits [data-md-bit="0"]');assert.match($('md-control-gates').textContent,/R\[0\]=1/);
  await exportSvg('md-diagram','divide-keep-subtraction');await exportSvg('md-control-gates','divider-remainder-mux-bit-0');await exportSvg('md-gate-diagram','divider-subtractor-bit-0');
  $('next-event').click();advance(5);
  assert.equal($('alu-output').textContent,'fffffffd');assert.match($('muldiv-signals').textContent,/Remainderfffffffe/);
  $('md-circuit').value='1';change('md-circuit');assert.match($('md-circuit-summary').textContent,/00000002 = fffffffe/);
  $('step').click();assert.equal(document.querySelector('[data-register="5"]').lastElementChild.textContent,'fffffffd');
  $('md-next').click();advance(5);$('next-event').click();advance(5);
  assert.equal($('alu-output').textContent,'fffffffe');$('step').click();

  // Follow a cold load through each resolved lookup, then each separate refill.
  $('stop-event').value='32';$('next-event').click();advance(5);
  assert.equal($('cache-select').value,'1');assert.match($('tag-compare').textContent,/MISS/);
  assert.match($('cache-stage-label').textContent,/L2 lookup/);
  assert.equal($('circuit-data').textContent,'pending');
  for(const [level,stage] of [['2','L3 lookup'],['3','RAM read'],['3','L3 refill'],['3','L2 refill'],['2','L1D refill'],['1','Complete']]){
    $('cache-next').click();advance(5);assert.equal($('cache-select').value,level);assert.ok($('cache-stage-label').textContent.includes(stage),$('cache-stage-label').textContent);
  }
  assert.match($('tag-compare').textContent,/MISS/,'a completed refill must not retroactively become a hit');
  assert.equal($('circuit-data').textContent,'0000002a');
  assert.equal($('line-bytes').querySelectorAll('.byte').length,64);
  assert.equal(document.querySelector('[data-register="13"]').lastElementChild.textContent,'00000000');
  $('cache-next').click();advance(5);
  assert.equal($('cache-select').value,'1');assert.match($('tag-compare').textContent,/HIT/);assert.equal($('circuit-data').textContent,'00000063');
  assert.equal(document.querySelector('[data-register="13"]').lastElementChild.textContent,'0000002a');
  assert.equal($('cache-journey').children.length,1,'warm L1 hit has one lookup stage');
  // A store exposes the old selected bytes and new payload independently; bytes
  // only change when the write-through finishes, not when its lookup hits.
  $('cache-next').click();advance(5);
  assert.match($('cache-stage-label').textContent,/RAM write-through/);
  assert.match($('tag-compare').textContent,/read 00000000.*store 00000008/);
  assert.equal($('line-bytes').querySelectorAll('.byte')[8].textContent,'00');
  const beforeStore=Number($('cycles').textContent.replaceAll(',',''));
  $('cache-next').click();advance(5);
  assert.equal(Number($('cycles').textContent.replaceAll(',',''))-beforeStore,60);
  assert.equal($('line-bytes').querySelectorAll('.byte')[8].textContent,'08');
  assert.equal($('circuit-data').textContent,'00000000','lookup data retains the old value');
  $('event-filter').value='register_write';change('event-filter');
  assert.ok(document.querySelectorAll('#event-list [data-event="register_write"]').length>0);
  click('[data-register="13"]');assert.match($('selected-register').textContent,/a3/);
  assert.equal($('register-bits').children.length,32);
  $('register-format').value='unsigned';change('register-format');assert.equal(document.querySelector('[data-register="13"]').lastElementChild.textContent,'42');
  $('register-format').value='hex';change('register-format');
  click('[data-inspector="ram"]');assert.equal($('view-ram').hidden,false);assert.equal($('memory-bytes').querySelectorAll('.byte').length,128);

  await example('array_sum');$('to-main').click();advance(30);assert.equal($('symbol-label').textContent,'main');
  assert.equal($('to-main').disabled,true);
  click('.asm-line.active .breakpoint');assert.equal(document.querySelector('.breakpoint.set').getAttribute('aria-pressed'),'true');
  $('speed').value='4';$('play').click();advance(100);assert.match($('status').textContent,/Exited/);
  assert.equal($('console').textContent,'First pass:  136\nSecond pass: 136\n');
  click('[data-cache="1"]');assert.equal($('cache-select').value,'1');assert.ok(document.querySelectorAll('#cache-lines tr.valid').length>0);
  $('back').click();assert.equal($('status').textContent,'Paused');
  $('reset').click();assert.equal($('console').textContent,'');click('[data-cache="1"]');assert.equal(document.querySelectorAll('#cache-lines tr.valid').length,0);
  await example('sort');$('play').click();advance(150);assert.match($('console').textContent,/2 3 5 7 11 18 29 42 55 64 76 91/);
  await example('echo');$('terminal-input').value='hello from UI';$('input-form').dispatchEvent(new dom.window.Event('submit',{bubbles:true,cancelable:true}));
  advance(100);assert.equal($('console').textContent,'> hello from UI\n');

  // Follow one real C array element, with the engine driving every visual.
  await example('flow');
  assert.match($('watch-label').textContent,/samples\[2\].*= 42/);
  assert.equal($('watch-array').children.length,4);
  assert.equal($('diagram').querySelectorAll('[data-cache]').length,4,'caches belong to the same zoomable machine');
  const fit=$('diagram').getAttribute('viewBox');$('zoom-in').click();assert.notEqual($('diagram').getAttribute('viewBox'),fit);
  click('[data-focus="machine"]');assert.equal($('diagram').getAttribute('viewBox'),fit);
  $('watch-go').click();advance(10);
  assert.match($('status').textContent,/watched value/);
  assert.match($('watch-state').textContent,/Address ready|lookup|refill|read/);
  const first=Number($('cycle-target').value);
  for(let n=0;n<20&&!/execute \d+\/34/.test($('watch-state').textContent);n++){$('watch-go').click();advance(4);}
  assert.match($('watch-state').textContent,/execute \d+\/34/);
  assert.match($('md-action').textContent,/./);
  const mid=Number($('cycle-target').value),beforeMid=$('watch-state').textContent;
  assert.ok(mid>first);assert.ok(document.querySelectorAll('#value-graph .value-node').length>=2);
  assert.ok(document.querySelectorAll('#registers .tracked').length>0);
  assert.equal($('value-locations').querySelectorAll('.resident').length,5,'RAM, three data caches, and a register contain the selected value');
  assert.equal($('view-alu').hidden,false);assert.equal($('muldiv-view').hidden,false);
  $('cycle-back').click();assert.equal(Number($('cycle-target').value),mid-1);
  $('to-frontier').click();assert.equal(Number($('cycle-target').value),mid);assert.equal($('watch-state').textContent,beforeMid);
  assert.match($('md-action').textContent,/Load working registers/,'the inspector must redraw after seeking before operand preparation');
  $('cycle-scrubber').value=first;$('cycle-scrubber').dispatchEvent(new dom.window.Event('input',{bubbles:true}));
  assert.equal(Number($('cycle-target').value),first);
  assert.equal(document.querySelectorAll('#value-graph .value-node').length,0,'future writes disappear on rewind');
  $('to-frontier').click();assert.equal(Number($('cycle-target').value),mid);
  for(let n=0;n<35&&!document.querySelector('.latch-summary.writing');n++){$('watch-go').click();advance(3);}
  assert.ok(document.querySelector('.latch-summary.writing'),'a real register clock edge enables the write');
  assert.equal($('latch-d').textContent,'0000007e');assert.equal($('latch-q').textContent,'0000007e');
  assert.match($('value-graph').textContent,/mul/);
  const committed=Number($('cycle-target').value);
  $('cycle-back').click();assert.equal($('latch-q').textContent,'0000002a');assert.equal($('latch-d').textContent,'0000007e');
  assert.match($('latch-clock').textContent,/WE=0/);
  $('cycle-forward').click();assert.equal(Number($('cycle-target').value),committed);assert.equal($('latch-q').textContent,'0000007e');
  click('[data-focus="machine"]');
  await exportSvg('diagram','flow-multiply-writeback');await exportSvg('waveform','clock-register-write');await exportSvg('value-graph','array-value-origins');
  $('speed').value='4';$('play').click();advance(100);assert.equal($('console').textContent,'126\n');
  assert.equal($('output-bytes').children.length,4);click('#output-bytes [data-output="0"]');
  assert.match($('trace-selection').textContent,/UART output byte 0/);
  const outputNode=$('value-graph').querySelector('.root[data-origin]');outputNode.dispatchEvent(new dom.window.MouseEvent('click',{bubbles:true}));
  assert.match($('trace-detail').textContent,/stored value/);
  $('trace-memory').click();assert.match($('trace-selection').textContent,/RAM/);
  click('[data-location="1"]');click('#line-bytes .byte');assert.equal($('watch-size').value,'1');

  const file={name:'invalid.elf',size:5,arrayBuffer:async()=>new Uint8Array([1,2,3,4,5]).buffer};
  Object.defineProperty($('upload'),'files',{value:[file],configurable:true});change('upload');await until(()=>!$('error').hidden);assert.match($('error').textContent,/ELF/);
  const badHeaders=new Uint8Array(await readFile(new URL('programs/array_sum.elf',base)));new DataView(badHeaders.buffer).setUint32(32,0xfffffffe,true);
  Object.defineProperty($('upload'),'files',{value:[{name:'bad-headers.elf',size:badHeaders.length,arrayBuffer:async()=>badHeaders.buffer}],configurable:true});
  $('error').hidden=true;change('upload');await until(()=>!$('error').hidden);assert.match($('error').textContent,/ELF section header/);
  dom.window.close();
});
