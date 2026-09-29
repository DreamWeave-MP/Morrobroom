document.addEventListener('DOMContentLoaded', function() {
  const docsShell = document.querySelector('.docs-shell');
  if (!docsShell) {
    return;
  }

  // Below 768px the navigation, and below 1200px the page's contents, fold into drawers (see
  // docs.sass). They start closed, so the page's own title and text come first; on a wider
  // screen they stay open beside the page.
  const panels = [
    [docsShell.querySelector('.docs-sidebar__panel'), window.matchMedia('(max-width: 767px)')],
    [docsShell.querySelector('.docs-toc__panel'), window.matchMedia('(max-width: 1199px)')],
  ].filter(function(entry) {
    return entry[0];
  });

  panels.forEach(function(entry) {
    const panel = entry[0];
    const narrowScreen = entry[1];
    const syncPanel = function() {
      if (narrowScreen.matches) {
        panel.removeAttribute('open');
      } else {
        panel.setAttribute('open', '');
      }
    };
    syncPanel();
    narrowScreen.addEventListener?.('change', syncPanel);
  });
  docsShell.classList.add('docs-shell--ready');

  // A drawer closes when a link in it is followed, on a click outside it, and on Escape.
  const openDrawers = function() {
    return panels.filter(function(entry) {
      return entry[1].matches && entry[0].open;
    }).map(function(entry) {
      return entry[0];
    });
  };
  document.addEventListener('click', function(event) {
    openDrawers().forEach(function(panel) {
      if (panel.contains(event.target) && !event.target.closest('a')) {
        return;
      }
      panel.removeAttribute('open');
    });
  });
  document.addEventListener('keydown', function(event) {
    if (event.key !== 'Escape') {
      return;
    }
    openDrawers().forEach(function(panel) {
      panel.removeAttribute('open');
      panel.querySelector('summary')?.focus();
    });
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
