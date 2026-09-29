// DreamWeave Mod Template: progressive enhancement. Every page works without this file;
// it adds search, copy buttons, the narrow header's search and menu buttons, the screenshot
// viewer, the openmw.cfg configurator, and marks the visitor's own platform among a program's
// downloads.
(() => {
  'use strict';

  const script = document.currentScript;

  function readStorage(key) {
    try { return window.localStorage.getItem(key); } catch { return null; }
  }

  function writeStorage(key, value) {
    try { window.localStorage.setItem(key, value); } catch { /* private mode or blocked: fine */ }
  }

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      const area = document.createElement('textarea');
      area.value = text;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.append(area);
      area.select();
      const copied = document.execCommand('copy');
      area.remove();
      return copied;
    }
  }

  function flash(button, text) {
    const original = button.dataset.label || button.textContent;
    button.dataset.label = original;
    button.textContent = text;
    window.setTimeout(() => { button.textContent = original; }, 1400);
  }

  // Copy buttons ------------------------------------------------------------------------------

  document.addEventListener('click', async event => {
    const button = event.target.closest('[data-copy], [data-copy-code]');
    if (!button) return;
    const text = button.hasAttribute('data-copy')
      ? button.getAttribute('data-copy')
      : button.parentElement.querySelector('code').textContent;
    flash(button, await copyText(text) ? 'Copied' : 'Copy failed');
  });

  // A code block goes in a frame that holds its copy button, so the button stays in the corner
  // while the code scrolls sideways under it.
  for (const code of document.querySelectorAll('pre > code')) {
    const block = code.parentElement;
    const frame = document.createElement('div');
    frame.className = 'dw-code';
    block.before(frame);
    frame.append(block);
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'dw-copy';
    button.textContent = 'Copy';
    button.setAttribute('data-copy-code', '');
    button.setAttribute('aria-label', 'Copy code to clipboard');
    frame.append(button);
  }

  // A Markdown table gets the frame the template's own tables have: it scrolls inside it, and
  // spans the column when it is narrower.
  for (const table of document.querySelectorAll('.dw-prose table, .docs-article table')) {
    if (table.parentElement.classList.contains('dw-table-scroll')) continue;
    const frame = document.createElement('div');
    frame.className = 'dw-table-scroll';
    table.before(frame);
    frame.append(table);
  }

  // Header --------------------------------------------------------------------------------------
  // Below 900px the search waits behind a button, and a menu of four links or more behind
  // another. Without this script both stay in view.

  const header = document.querySelector('.dw-header');
  const headerTools = header?.querySelector('[data-header-tools]');
  const searchToggle = headerTools?.querySelector('[data-search-toggle]');
  const menuToggle = headerTools?.querySelector('[data-menu-toggle]');
  const setOpen = (button, state, open) => {
    if (!button) return;
    header.classList.toggle(state, open);
    button.setAttribute('aria-expanded', String(open));
  };
  if (headerTools) {
    if (searchToggle && !header.querySelector('#dw-search-input')) searchToggle.remove();
    headerTools.hidden = false;
    searchToggle?.addEventListener('click', () => {
      const open = !header.classList.contains('is-searching');
      setOpen(searchToggle, 'is-searching', open);
      setOpen(menuToggle, 'is-menu-open', false);
      if (open) header.querySelector('#dw-search-input')?.focus();
    });
    menuToggle?.addEventListener('click', () => {
      setOpen(menuToggle, 'is-menu-open', !header.classList.contains('is-menu-open'));
      setOpen(searchToggle, 'is-searching', false);
    });
    header.addEventListener('keydown', event => {
      if (event.key !== 'Escape' || !header.classList.contains('is-menu-open')) return;
      setOpen(menuToggle, 'is-menu-open', false);
      menuToggle.focus();
    });
  }

  // Search ------------------------------------------------------------------------------------

  const searchInput = document.getElementById('dw-search-input');
  if (searchInput && script && script.dataset.searchIndex) {
    const results = document.getElementById('dw-search-results');
    const status = results.querySelector('.dw-search__status');
    const list = results.querySelector('.dw-search__list');
    const scope = new URL(searchInput.dataset.scope || '/', window.location.href).pathname;
    let index = null;
    let request = 0;

    const loadIndex = () => {
      index ??= fetch(script.dataset.searchIndex)
        .then(response => {
          if (!response.ok) throw new Error(`search index returned ${response.status}`);
          return response.json();
        })
        .catch(error => {
          index = null;
          throw error;
        });
      return index;
    };

    const highlight = (element, text, words) => {
      const pattern = new RegExp(`(${words.map(word => word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi');
      for (const piece of text.split(pattern)) {
        if (!piece) continue;
        if (words.some(word => piece.toLocaleLowerCase() === word)) {
          const mark = document.createElement('mark');
          mark.textContent = piece;
          element.append(mark);
        } else {
          element.append(piece);
        }
      }
    };

    const excerpt = (body, words) => {
      const lower = body.toLocaleLowerCase();
      const position = Math.max(0, Math.min(...words.map(word => {
        const found = lower.indexOf(word);
        return found < 0 ? Infinity : found;
      })));
      const start = Number.isFinite(position) ? Math.max(0, position - 60) : 0;
      return (start > 0 ? '…' : '') + body.slice(start, start + 160).trim() + '…';
    };

    const render = async () => {
      const current = ++request;
      const query = searchInput.value.trim().toLocaleLowerCase();
      list.replaceChildren();
      results.hidden = !query;
      if (!query) return;

      status.textContent = 'Searching…';
      let records;
      try {
        records = await loadIndex();
      } catch {
        status.textContent = 'Search is unavailable here. The navigation still works.';
        return;
      }
      if (current !== request) return;

      const words = query.split(/\s+/).filter(Boolean);
      const matches = records
        .filter(record => new URL(record.url, window.location.href).pathname.startsWith(scope))
        .map(record => {
          const title = (record.title || '').toLocaleLowerCase();
          const text = `${title} ${(record.description || '').toLocaleLowerCase()} ${(record.body || '').toLocaleLowerCase()}`;
          if (!words.every(word => text.includes(word))) return null;
          const score = words.reduce((total, word) => total + (title.includes(word) ? 10 : 0) + (title.startsWith(word) ? 5 : 0), 0);
          return { record, score };
        })
        .filter(Boolean)
        .sort((left, right) => right.score - left.score)
        .slice(0, 12);

      status.textContent = matches.length
        ? `${matches.length} result${matches.length === 1 ? '' : 's'}${matches.length === 12 ? ' (first 12)' : ''}`
        : 'Nothing matches. Try fewer words.';

      for (const { record } of matches) {
        const item = document.createElement('li');
        const link = document.createElement('a');
        link.href = record.url;
        const title = document.createElement('strong');
        highlight(title, record.title || record.url, words);
        const snippet = document.createElement('span');
        highlight(snippet, excerpt(record.description || record.body || '', words), words);
        link.append(title, snippet);
        item.append(link);
        list.append(item);
      }
    };

    let timer = 0;
    searchInput.addEventListener('input', () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(render, 120);
    });
    searchInput.addEventListener('focus', loadIndex, { once: true });
    searchInput.addEventListener('keydown', event => {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        list.querySelector('a')?.focus();
      } else if (event.key === 'Escape') {
        results.hidden = true;
        searchInput.blur();
        if (header?.classList.contains('is-searching')) {
          setOpen(searchToggle, 'is-searching', false);
          searchToggle?.focus();
        }
      }
    });
    list.addEventListener('keydown', event => {
      const links = [...list.querySelectorAll('a')];
      const position = links.indexOf(document.activeElement);
      if (event.key === 'ArrowDown' && position < links.length - 1) {
        event.preventDefault();
        links[position + 1].focus();
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        (position > 0 ? links[position - 1] : searchInput).focus();
      } else if (event.key === 'Escape') {
        results.hidden = true;
        searchInput.focus();
      }
    });
    document.addEventListener('keydown', event => {
      const typing = ['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement?.tagName) || document.activeElement?.isContentEditable;
      if (event.key === '/' && !typing) {
        event.preventDefault();
        setOpen(searchToggle, 'is-searching', true);
        searchInput.focus();
      }
    });
    document.addEventListener('click', event => {
      if (!event.target.closest('.dw-search')) results.hidden = true;
    });
  }

  // Platform downloads ------------------------------------------------------------------------
  // A program has one archive per platform. Mark the ones for the visitor's platform; hide none,
  // because the guess can be wrong. Handheld builds (PortMaster, muOS) are never marked: no
  // browser says it is one. iPhones and iPads get no mark: no archive here runs on one.

  const platform = (() => {
    const hint = `${navigator.userAgentData?.platform || ''} ${navigator.platform || ''} ${navigator.userAgent || ''}`.toLowerCase();
    if (hint.includes('android')) return 'android';
    if (/iphone|ipad/.test(hint)) return null;
    if (hint.includes('win')) return 'windows';
    if (hint.includes('mac')) return 'macos';
    if (/linux|x11/.test(hint)) return 'linux';
    return null;
  })();
  if (platform) {
    for (const link of document.querySelectorAll(`a[data-platform="${platform}"]`)) link.classList.add('is-yours');
  }

  // Screenshot viewer -------------------------------------------------------------------------

  const dialog = document.querySelector('[data-lightbox-dialog]');
  if (dialog && typeof dialog.showModal === 'function') {
    const image = dialog.querySelector('[data-lightbox-image]');
    const caption = dialog.querySelector('[data-lightbox-caption]');
    const counter = dialog.querySelector('[data-lightbox-counter]');
    let items = [];
    let position = 0;
    let opener = null;

    const show = next => {
      position = (next + items.length) % items.length;
      const item = items[position];
      image.src = item.href;
      image.alt = item.dataset.alt || '';
      caption.textContent = item.dataset.caption || '';
      counter.textContent = `${position + 1} / ${items.length}`;
    };

    document.addEventListener('click', event => {
      const link = event.target.closest('a[data-lightbox]');
      if (!link || event.ctrlKey || event.metaKey || event.shiftKey || event.button !== 0) return;
      event.preventDefault();
      items = [...document.querySelectorAll(`a[data-lightbox="${CSS.escape(link.dataset.lightbox)}"]`)];
      opener = link;
      show(items.indexOf(link));
      dialog.showModal();
    });

    dialog.querySelector('[data-lightbox-previous]').addEventListener('click', () => show(position - 1));
    dialog.querySelector('[data-lightbox-next]').addEventListener('click', () => show(position + 1));
    dialog.querySelector('[data-lightbox-close]').addEventListener('click', () => dialog.close());
    dialog.addEventListener('keydown', event => {
      if (event.key === 'ArrowLeft') show(position - 1);
      if (event.key === 'ArrowRight') show(position + 1);
    });
    dialog.addEventListener('click', event => {
      if (event.target === dialog) dialog.close();
    });
    dialog.addEventListener('close', () => {
      image.src = 'data:,';
      opener?.focus();
    });
  }

  // openmw.cfg configurator -------------------------------------------------------------------

  const MODS_FOLDER_KEY = 'dreamweave.modsFolder';

  for (const configurator of document.querySelectorAll('[data-configurator]')) {
    const model = JSON.parse(configurator.querySelector('[data-install-model]').textContent);
    const output = configurator.querySelector('.dw-configurator__output code');
    const pathInput = configurator.querySelector('[data-install-path]');
    const choices = [...configurator.querySelectorAll('[data-component-choice]')];
    const section = configurator.closest('.dw-method__body') || configurator;

    for (const controls of section.querySelectorAll('[data-configurator-controls]')) controls.hidden = false;
    if (pathInput) {
      pathInput.placeholder = model.placeholder;
      pathInput.value = readStorage(MODS_FOLDER_KEY) || '';
    }

    const selected = () => {
      const chosen = new Set(model.components.filter(component => component.required).map(component => component.id));
      for (const choice of choices) {
        if (choice.checked && choice.value) chosen.add(choice.value);
      }
      return chosen;
    };

    const update = () => {
      const folder = (pathInput?.value.trim() || model.placeholder).replace(/\\/g, '/').replace(/\/+$/, '');
      const root = `${folder}/${model.name}`;
      const chosen = selected();
      const lines = [];
      for (const block of model.config) {
        if (!chosen.has(block.component)) continue;
        for (const line of block.lines) lines.push(line.replaceAll('{install}', root));
      }
      output.textContent = lines.join('\n') + '\n';
      if (pathInput?.value.trim()) writeStorage(MODS_FOLDER_KEY, folder);
    };

    pathInput?.addEventListener('input', update);
    for (const choice of choices) choice.addEventListener('change', update);
    if (pathInput) {
      const label = configurator.querySelector(`label[for="${pathInput.id}"]`);
      if (label) label.textContent = 'Your mods folder';
    }
    update();
  }
})();
