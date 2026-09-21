document.addEventListener('DOMContentLoaded', function() {
  const docsShell = document.querySelector('.docs-shell');
  if (!docsShell) {
    return;
  }

  docsShell.querySelectorAll('.docs-article pre').forEach(function(block) {
    const code = block.querySelector('code');
    if (!code || block.parentElement.classList.contains('docs-code-block')) {
      return;
    }

    const wrapper = document.createElement('div');
    wrapper.className = 'docs-code-block';
    block.parentNode.insertBefore(wrapper, block);
    wrapper.appendChild(block);

    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'docs-code-copy';
    button.textContent = 'Copy';
    button.setAttribute('aria-label', 'Copy code to clipboard');
    wrapper.appendChild(button);

    button.addEventListener('click', async function() {
      let copied = false;
      try {
        await navigator.clipboard.writeText(code.textContent);
        copied = true;
      } catch (error) {
        try {
          const selection = window.getSelection();
          if (!selection) {
            throw new Error('Text selection is unavailable');
          }
          const range = document.createRange();
          range.selectNodeContents(code);
          selection.removeAllRanges();
          selection.addRange(range);
          copied = document.execCommand('copy');
          selection.removeAllRanges();
        } catch (fallbackError) {
          copied = false;
        }
      }

      button.textContent = copied ? 'Copied!' : 'Copy failed';
      button.setAttribute('aria-label', copied ? 'Code copied to clipboard' : 'Copying code failed');
      window.setTimeout(function() {
        button.textContent = 'Copy';
        button.setAttribute('aria-label', 'Copy code to clipboard');
      }, 1500);
    });
  });

  const sidebarPanel = docsShell.querySelector('.docs-sidebar__panel');
  const narrowScreen = window.matchMedia('(max-width: 899px)');
  const syncSidebar = function() {
    if (narrowScreen.matches) {
      sidebarPanel?.removeAttribute('open');
    } else {
      sidebarPanel?.setAttribute('open', '');
    }
  };

  syncSidebar();
  narrowScreen.addEventListener?.('change', syncSidebar);

  const search = document.getElementById('search');
  document.addEventListener('keydown', function(event) {
    if (event.key === '/' && document.activeElement !== search && !['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement.tagName)) {
      event.preventDefault();
      search?.focus();
    }

    if (event.key === 'Escape' && search) {
      search.blur();
      const results = document.querySelector('.search-results');
      if (results) {
        results.style.display = 'none';
      }
    }
  });

  const tocLinks = Array.from(docsShell.querySelectorAll('.docs-toc a'));
  const headings = tocLinks.map(function(link) {
    const id = new URL(link.href).hash.slice(1);
    return document.getElementById(id);
  }).filter(Boolean);

  if (!headings.length || !('IntersectionObserver' in window)) {
    return;
  }

  const observer = new IntersectionObserver(function(entries) {
    entries.forEach(function(entry) {
      if (!entry.isIntersecting) {
        return;
      }

      tocLinks.forEach(function(link) {
        link.classList.toggle('is-active', new URL(link.href).hash === `#${entry.target.id}`);
      });
    });
  }, { rootMargin: '-15% 0px -70% 0px' });

  headings.forEach(function(heading) {
    observer.observe(heading);
  });
});
