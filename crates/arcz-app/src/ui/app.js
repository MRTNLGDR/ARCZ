(() => {
  'use strict';
  const D = window.ARCZ_DATA || { projects: [], packages: [] };
  const $ = (id) => document.getElementById(id);
  const consoleEl = $('console');

  function show(value) {
    consoleEl.textContent = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
  }

  async function command(name, params = {}) {
    const response = await fetch(`/cmd/${encodeURIComponent(name)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    });
    const payload = await response.json();
    if (!response.ok || payload.ok === false) {
      throw new Error(payload.erro || `comando ${name} falhou`);
    }
    return payload.dado ?? payload;
  }

  function card(title, subtitle, badge) {
    const node = document.createElement('article');
    node.className = 'item-card';
    node.innerHTML = `<div><strong></strong><small></small></div><span></span>`;
    node.querySelector('strong').textContent = title || 'sem nome';
    node.querySelector('small').textContent = subtitle || '—';
    node.querySelector('span').textContent = badge || '';
    return node;
  }

  function renderCollections() {
    const projects = $('project-list');
    const packages = $('package-list');
    projects.replaceChildren();
    packages.replaceChildren();
    for (const project of D.projects || []) {
      projects.append(card(project.name, `${project.local || '—'} · ${project.type || 'ARCZ'}`, project.status));
    }
    for (const pack of D.packages || []) {
      packages.append(card(pack.name, pack.size || '—', pack.status));
    }
    if (!projects.children.length) projects.append(card('Nenhum projeto salvo', 'O runtime não reportou projetos', 'vazio'));
    if (!packages.children.length) packages.append(card('Nenhum pacote local', 'Materialize dados quando necessário', 'offline'));
  }

  function renderRuntime() {
    $('footer-scene').textContent = D.rodape || 'cena desconhecida';
    $('footer-contract').textContent = D.contrato || 'contrato desconhecido';
    const status = D.vivo && D.vivo.status;
    $('status-pill').textContent = status ? 'runtime conectado' : 'offline / aguardando';
    $('status-pill').classList.toggle('ok', Boolean(status));
    const metrics = $('runtime-summary');
    metrics.replaceChildren();
    const values = [
      ['Projetos', (D.projects || []).length],
      ['Pacotes', (D.packages || []).length],
      ['Objetos', status?.objetos ?? 0],
      ['Rede', status?.network_mode || 'offline_strict']
    ];
    for (const [label, value] of values) {
      const m = document.createElement('div');
      m.innerHTML = `<small></small><strong></strong>`;
      m.querySelector('small').textContent = label;
      m.querySelector('strong').textContent = String(value);
      metrics.append(m);
    }
  }

  async function refresh() {
    try {
      const [status, scene, capabilities] = await Promise.all([
        command('workspace.status'), command('scene.list'), command('capability.list')
      ]);
      D.vivo = { status, cena: scene, capacidades: capabilities };
      D.rodape = `${status.objetos ?? 0} objetos · ${status.modelo_carregado ? 'modelo carregado' : 'sem modelo'} · rede ${status.network_mode || 'offline_strict'}`;
      const implemented = capabilities.implementados?.length ?? 0;
      const total = capabilities.total_contrato ?? capabilities.total ?? 0;
      D.contrato = `${implemented}/${total} comandos ligados`;
      renderCollections();
      renderRuntime();
      show({ status, scene, capabilities });
    } catch (error) {
      $('status-pill').textContent = 'runtime indisponível';
      $('status-pill').classList.remove('ok');
      show(String(error));
    }
  }

  document.querySelectorAll('[data-cmd]').forEach((button) => {
    button.addEventListener('click', async () => {
      try { show(await command(button.dataset.cmd)); } catch (error) { show(String(error)); }
    });
  });
  $('run-command').addEventListener('click', async () => {
    try {
      const name = $('command-name').value.trim();
      const params = JSON.parse($('command-payload').value || '{}');
      show(await command(name, params));
    } catch (error) { show(String(error)); }
  });
  $('refresh').addEventListener('click', refresh);

  renderCollections();
  renderRuntime();
  refresh();
})();
