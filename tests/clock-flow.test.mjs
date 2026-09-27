import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {JSDOM} from 'jsdom';
import init, {Simulator} from '../web/pkg/visual_cpu.js';
import {clockFlow} from '../web/clock-flow.js';
import {drawClock} from '../web/clock-diagram.js';
import {createInspectors} from '../web/inspectors.js';

const base=new URL('../web/',import.meta.url);
await init({module_or_path:await readFile(new URL('pkg/visual_cpu_bg.wasm',base))});
const snapshot=sim=>JSON.parse(sim.snapshot());
const load=async name=>new Simulator(await readFile(new URL(`programs/${name}.elf`,base)));
const hex=n=>(n>>>0).toString(16).padStart(8,'0');

test('clock flow uses the completed memory stage and previews only recorded state',async()=>{
  const sim=await load('alu');
  const dom=new JSDOM(await readFile(new URL('index.html',base),'utf8'));
  globalThis.document=dom.window.document;
  const $=id=>document.getElementById(id);
  const inspectors=createInspectors({pause(){},nextCacheStage(){},nextMulDivCycle(){}});
  const draw=(state,step)=>{inspectors.render(state,sim);drawClock(state,step,clockFlow(state));};
  try{
    let state=snapshot(sim);
    assert.equal(state.cycle_detail.before,null);
    assert.equal(clockFlow(state).cards[2].value,'Empty');
    sim.tick(1);state=snapshot(sim);
    assert.equal(state.access.stage,'L2 lookup');
    assert.deepEqual(clockFlow(state).work,['L1I']);
    draw(state,1);
    assert.ok(document.querySelector('[data-cache="0"]').classList.contains('clock-work'));
    assert.ok(!document.querySelector('[data-cache="2"]').classList.contains('active'));
    sim.tick(25);state=snapshot(sim);
    assert.equal(state.cycle_detail.memory_clock.remaining_before,60);
    assert.equal(state.cycle_detail.memory_clock.remaining_after,59);
    assert.deepEqual(state.cycle_detail.before,state.cycle_detail.after);
    assert.deepEqual(clockFlow(state).destinations,[]);
    assert.equal(clockFlow(state).cards[2].name,'No data transfer');
    draw(state,1);
    assert.equal(document.querySelectorAll('.memory-wire.active,.memory-wire.returning').length,0);
    assert.equal(document.querySelectorAll('.memory-wire.clock-held-wire').length,1);
    const waiting=state.cycle_detail;
    sim.tick(60);state=snapshot(sim);
    assert.equal(state.cycle,86);
    assert.equal(state.access.stage,'L2 refill');
    assert.deepEqual(clockFlow(state).work,['L3']);
    draw(state,0);assert.equal($('cache-label-3').textContent,'0 / 128 valid');
    draw(state,1);assert.equal($('cache-label-3').textContent,'0 / 128 valid');
    draw(state,2);assert.equal($('cache-label-3').textContent,'1 / 128 valid');
    sim.tick(2);state=snapshot(sim);
    draw(state,0);assert.equal($('instruction-word').textContent,'—');
    draw(state,1);assert.equal($('instruction-word').textContent,'—');
    draw(state,2);assert.equal($('instruction-word').textContent,hex(state.instruction.raw));
    sim.tick(2);state=snapshot(sim);
    assert.equal(state.cycle_detail.phase,'Execute');
    assert.equal(state.cycle_detail.before.alu_result,null);
    draw(state,0);assert.equal($('execute-detail').textContent,'—');
    draw(state,1);assert.equal($('execute-detail').textContent,hex(state.cycle_detail.after.alu_result));
    sim.tick(2);state=snapshot(sim);
    assert.equal(state.cycle_detail.phase,'Writeback');
    assert.ok(!('events' in state),'the presentation must not depend on private engine event flags');
    const rd=state.last_written,before=state.cycle_detail.before.regs[rd],after=state.cycle_detail.after.regs[rd];
    assert.notEqual(before,after);
    assert.equal(clockFlow(state).cards[1].value,'Write enable = 1');
    assert.ok(clockFlow(state).edgeWires.includes('wire-writeback'));
    for(const step of [0,1,2]){
      draw(state,step);
      const register=document.querySelector(`[data-register="${rd}"]`);
      assert.equal(register.lastElementChild.textContent,hex(step===2?after:before));
      assert.equal(register.classList.contains('changed'),step===2);
      assert.match($('write-enable').textContent,step===0?/WE = 0/:/WE = 1/);
    }
    assert.ok(sim.seek(26));assert.deepEqual(snapshot(sim).cycle_detail,waiting);
  }finally{sim.free();dom.window.close();}
});

test('clock flow has no unfinished multiply result or nonexistent control/register transfer',async()=>{
  const sim=await load('alu');
  let pending=false,finished=false,control=false,unusedDecode=false,multiplyResult,loadResult;
  try{
    for(let n=0;n<3000;n++){
      sim.tick(1);const state=snapshot(sim),c=state.cycle_detail,p=state.datapath,flow=clockFlow(state);
      assert.equal(state.fault,null);
      if(c.phase==='Decode'&&p){
        if(!p.read_a&&p.left_source!=='PC'){
          assert.ok(!flow.workWires.includes('wire-read-a'));unusedDecode=true;
        }
        if(!p.read_b&&p.right_source!=='immediate')assert.ok(!flow.workWires.includes('wire-read-b'));
      }
      if(c.phase==='Execute'&&p?.alu?.muldiv){
        if(p.alu.ready){assert.ok(flow.edgeWires.includes('wire-muldiv-result'));finished=true;multiplyResult??=state;}
        else{
          assert.equal(c.after.alu_result,null);
          assert.ok(!flow.edgeWires.includes('wire-muldiv-result'));
          assert.ok(!flow.destinations.includes('write'));pending=true;
        }
      }
      if(c.phase==='Execute'&&p&&!p.alu){
        assert.ok(!flow.edgeWires.includes('wire-result'));
        assert.ok(!flow.edgeWires.includes('wire-write-data'));
        assert.ok(!flow.destinations.includes('write'));control=true;
      }
      if(c.phase==='Memory'&&state.access?.kind==='load'&&state.access.complete)loadResult??=state;
      if(state.halted)break;
    }
    assert.ok(pending&&finished&&control&&unusedDecode,'all relevant real-program cases were exercised');
    assert.ok(loadResult,'the real assembly program completes a load');
    const dom=new JSDOM(await readFile(new URL('index.html',base),'utf8'));
    globalThis.document=dom.window.document;
    const inspectors=createInspectors({pause(){},nextCacheStage(){},nextMulDivCycle(){}});
    const $=id=>document.getElementById(id);
    try{
      inspectors.render(multiplyResult,sim);
      drawClock(multiplyResult,0,clockFlow(multiplyResult));
      assert.ok(!$('muldiv-value').textContent.includes(hex(multiplyResult.datapath.alu.result)));
      drawClock(multiplyResult,1,clockFlow(multiplyResult));
      assert.ok($('muldiv-value').textContent.includes(hex(multiplyResult.datapath.alu.result)));
      inspectors.render(loadResult,sim);
      drawClock(loadResult,0,clockFlow(loadResult));
      assert.equal($('memory-value').textContent,'load pending');
      drawClock(loadResult,1,clockFlow(loadResult));
      assert.equal($('memory-value').textContent,`load ${hex(loadResult.cycle_detail.after.write_value)}`);
    }finally{dom.window.close();}
  }finally{sim.free();}
  const branchSim=await load('flow');
  try{
    let branch;
    for(let n=0;n<200&&!branch;n++){
      branchSim.run_until(10000,new Uint32Array(),false,8);
      const state=snapshot(branchSim);
      assert.equal(state.fault,null);
      if(state.cycle_detail.phase==='Execute'&&state.datapath?.branch_taken!=null)branch=state;
      if(state.halted)break;
    }
    assert.ok(branch,'the real C program executes a branch');
    const flow=clockFlow(branch);
    assert.ok(!flow.destinations.includes('write'));
    assert.ok(!flow.edgeWires.includes('wire-write-data'));
  }finally{branchSim.free();}
});
