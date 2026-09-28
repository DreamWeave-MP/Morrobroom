document.addEventListener('DOMContentLoaded', function() {
  const docsShell = document.querySelector('.docs-shell');
  if (!docsShell) {
    return;
  }

  const sidebarPanel = docsShell.querySelector('.docs-sidebar__panel');
  const narrowScreen = window.matchMedia('(max-width: 1100px)');
  const syncSidebar = function() {
    if (narrowScreen.matches) {
      sidebarPanel?.removeAttribute('open');
    } else {
      sidebarPanel?.setAttribute('open', '');
    }
  };

  syncSidebar();
  narrowScreen.addEventListener?.('change', syncSidebar);

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
