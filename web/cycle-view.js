import {$,hex,escape} from './inspectors.js';
import {clockFlow} from './clock-flow.js';
import {drawClock} from './clock-diagram.js';

export function createCycleView({pause,inspect,finishWait}) {
  let state,step=2,lastCycle=-1,playing=false,started=0;
  const inputHTML=inputs=>inputs.map(v=>`<div class="cycle-input"><span>${escape(v.name)}</span><code>${escape(v.value)}</code></div>`).join('');
  function render(next) {
    state=next;const detail=state.cycle_detail;if(!detail)return;
    if(state.cycle!==lastCycle){step=state.cycle?2:0;lastCycle=state.cycle;playing=false;}
    $('cycle-caption').textContent=state.cycle?`Clock ${state.cycle} · ${detail.phase}`:'Before clock 1';
    $('instruction-age').textContent=detail.instruction_cycle?`instruction clock ${detail.instruction_cycle}`:'';
    $('cycle-instruction').textContent=`${hex(detail.pc)} · ${detail.instruction}`;
    $('cycle-title').textContent=detail.title;$('cycle-next').textContent=detail.next;
    $('cycle-inspect').disabled=!detail.phase;
    $('finish-wait').hidden=!detail.waiting||state.halted;
    const progress=detail.progress;$('cycle-progress').hidden=!progress;
    if(progress){$('cycle-progress-label').textContent=progress.name;$('cycle-progress-count').textContent=`${progress.elapsed} / ${progress.total} clocks`;$('cycle-progress-bar').max=progress.total;$('cycle-progress-bar').value=progress.elapsed;}
    document.querySelectorAll('[data-subcycle]').forEach(button=>{const active=Number(button.dataset.subcycle)===step;button.classList.toggle('active',active);button.setAttribute('aria-pressed',String(active));});
    const changes=detail.changes.map(v=>`<div class="cycle-change"><span>${escape(v.name)}</span><div><code>${escape(v.before||'empty')}</code><b>→</b><code>${escape(v.after)}</code></div></div>`).join('');
    const contents=[`<div class="cycle-inputs">${inputHTML(detail.inputs)}</div><p class="cycle-hold">Values presented to the active unit before this clock's update.</p>`,`<p class="cycle-reason">${escape(detail.reason)}</p><button id="cycle-open-logic">Open active unit →</button>`,`<p class="cycle-reason compact-reason">${escape(detail.reason)}</p>${changes||'<p class="cycle-no-change">No data value latched on this clock.</p>'}<p class="cycle-hold">${escape(detail.held)}</p>`];
    $('cycle-body').innerHTML=`<div class="inside-cards">${contents.map((html,i)=>`<article class="inside-card ${step===i?'active':''}" data-inside-card="${i}"><h3>${['Inputs before the update','Combinational work','Latched at the edge'][i]}</h3>${html}</article>`).join('')}</div>`;
    $('cycle-open-logic').onclick=inspectEdge;
    $('cycle-display-note').textContent=`${['Inputs at the start','Combinational work','State latched at the edge'][step]} · display replay; clock ${state.cycle} stays selected`;
    const flow=clockFlow(state);
    $('clock-focus-title').textContent=state.cycle?`Clock ${state.cycle} · ${detail.phase}`:'Before clock 1';
    $('clock-focus-instruction').textContent=`0x${hex(detail.pc)} · ${detail.instruction}`;
    $('clock-playback-note').textContent=state.cycle?`${playing?'Replaying':'Inspecting'} ${['inputs','logic','edge'][step]} · recorded clock ${state.cycle}`:'No clock has run';
    $('clock-replay').textContent=playing?'Stop replay':'Replay clock';
    $('clock-replay').disabled=$('clock-next-signal').disabled=$('clock-inspect').disabled=!state.cycle;
    $('clock-next-signal').textContent=step===2?'Back to inputs':'Next signal →';
    $('clock-held-summary').textContent=detail.waiting?`${detail.memory_clock.component} latency ${detail.memory_clock.remaining_before} → ${detail.memory_clock.remaining_after}. No modeled data transfer. ${detail.held}`:detail.held;
    $('clock-skip-wait').hidden=!detail.waiting||state.halted;
    document.querySelector('.clock-focus').classList.toggle('waiting',detail.waiting);
    $('clock-flow').innerHTML=flow.cards.map((card,i)=>`${i?'<span class="clock-flow-arrow" aria-hidden="true">→</span>':''}<button class="clock-flow-step ${i===step?'active':i<step?'passed':'future'}" data-clock-step="${i}" aria-pressed="${i===step}"><small class="clock-flow-label">${['1 · Inputs','2 · Logic','3 · Edge'][i]}</small><strong class="clock-flow-name">${escape(card.name)}</strong><code class="clock-flow-value">${escape(card.value)}</code><span class="clock-flow-detail">${escape(card.detail)}</span></button>`).join('');
    drawClock(state,step,flow);
    textPhase(detail);
  }
  function textPhase(detail){
    document.querySelectorAll('[data-phase]').forEach(el=>{el.classList.toggle('active',el.dataset.phase===detail.phase);el.title=el.dataset.phase===detail.phase?`This phase ran on clock ${state.cycle}`:'';});
    $('phase-caption').textContent=detail.phase?`Clock ${state.cycle}: ${['before edge','logic evaluated','after edge'][step]} · next: ${state.halted?'stopped':state.phase}`:'Before first fetch';
    $('trace-status').textContent=detail.waiting?'Waiting':detail.phase?`Clock ${state.cycle} · ${detail.phase}`:'Ready';
    $('current-instruction').textContent=`${hex(detail.pc)} · ${detail.instruction}`;
  }
  function select(next){if(!state)return;playing=false;pause();step=next;render(state);}
  function inspectEdge(){if(!state)return;select(2);inspect(state.cycle_detail.inspector);}
  function replay(pauseMachine=true){if(!state?.cycle)return;if(pauseMachine)pause();step=0;playing=true;started=performance.now();render(state);}
  document.querySelectorAll('[data-subcycle]').forEach(button=>button.onclick=()=>select(Number(button.dataset.subcycle)));
  $('clock-flow').onclick=e=>{const button=e.target.closest('[data-clock-step]');if(button)select(Number(button.dataset.clockStep));};
  $('clock-replay').onclick=()=>{if(playing){playing=false;render(state);}else replay();};
  $('clock-next-signal').onclick=()=>select((step+1)%3);
  $('clock-inspect').onclick=inspectEdge;
  $('clock-skip-wait').onclick=()=>state?.cycle_detail.waiting&&finishWait();
  $('cycle-inspect').onclick=inspectEdge;
  $('finish-wait').onclick=()=>state?.cycle_detail.waiting&&finishWait();
  return {render,replay,edge(){playing=false;step=2;if(state)render(state);},animate(time){if(!playing)return;const next=Math.max(0,Math.min(2,Math.floor((time-started)/750)));if(next!==step){step=next;if(next===2)playing=false;render(state);}},reset(){lastCycle=-1;step=2;playing=false;}};
}
