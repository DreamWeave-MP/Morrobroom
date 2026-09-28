document.addEventListener('DOMContentLoaded', function() {
  const docsShell = document.querySelector('.docs-shell');
  if (!docsShell) {
    return;
  }

  // On a narrow screen both panels sit above the article, so they start closed: the page's own
  // title and text come first.
  const panels = docsShell.querySelectorAll('.docs-sidebar__panel, .docs-toc__panel');
  const narrowScreen = window.matchMedia('(max-width: 1100px)');
  const syncPanels = function() {
    panels.forEach(function(panel) {
      if (narrowScreen.matches) {
        panel.removeAttribute('open');
      } else {
        panel.setAttribute('open', '');
      }
    });
  };

  syncPanels();
  narrowScreen.addEventListener?.('change', syncPanels);

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
