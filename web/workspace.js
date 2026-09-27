import {$} from './inspectors.js';

export function createWorkspace(){
  const column=document.querySelector('.main-column'),metrics=document.querySelector('.metrics');
  const format=$('register-format').closest('label');format.classList.add('register-format-control');
  $('register-bits-toggle').before(format);
  metrics.append(document.querySelector('.execution-status'));document.querySelector('.summary-row').hidden=true;
  const dock=document.createElement('div');dock.className='instrument-dock';dock.id='instrument-dock';
  const scope=document.createElement('section');scope.className='scope panel';
  scope.innerHTML='<div class="scope-tabs" role="tablist" aria-label="History view"><button data-scope="signals" class="active" role="tab" aria-selected="true">Signals</button><button data-scope="values" role="tab" aria-selected="false">Values</button><button data-scope="events" role="tab" aria-selected="false">Events</button><button id="expand-scope" aria-expanded="false">Expand</button></div>';
  const panels={signals:document.querySelector('.timeline'),values:document.querySelector('.provenance'),events:document.querySelector('.history-panel')};
  Object.values(panels).forEach(panel=>scope.append(panel));
  dock.append(document.querySelector('.cycle-explanation'),scope,document.querySelector('.console-panel'));
  document.querySelector('.bottom-grid').remove();column.append(dock);
  function showTab(name){for(const [key,panel] of Object.entries(panels))panel.hidden=key!==name;scope.querySelectorAll('[data-scope]').forEach(button=>{const active=button.dataset.scope===name;button.classList.toggle('active',active);button.setAttribute('aria-selected',String(active));});}
  scope.querySelectorAll('[data-scope]').forEach(button=>button.onclick=()=>showTab(button.dataset.scope));showTab('signals');
  function expand(kind){
    const expanded=!dock.classList.contains(`expanded-${kind}`);
    for(const name of ['scope','cycle']){const active=expanded&&kind===name;dock.classList.toggle(`expanded-${name}`,active);$(`expand-${name}`).textContent=active?'Compact':'Expand';$(`expand-${name}`).setAttribute('aria-expanded',String(active));}
    column.classList.toggle('scope-expanded',expanded);window.dispatchEvent(new window.Event('scope-layout'));
  }
  $('expand-scope').onclick=()=>expand('scope');$('expand-cycle').onclick=()=>expand('cycle');
  $('fit-layout').onclick=()=>{const fitted=document.body.classList.toggle('fit-workspace');$('fit-layout').setAttribute('aria-pressed',String(fitted));};
  $('toggle-code').onclick=()=>{const hidden=document.body.classList.toggle('hide-code');$('toggle-code').setAttribute('aria-pressed',String(!hidden));};
  $('register-bits-toggle').onclick=()=>{const open=document.querySelector('.register-inspection').classList.toggle('show-bits');$('register-bits-toggle').setAttribute('aria-expanded',String(open));};
  return {showTab};
}
