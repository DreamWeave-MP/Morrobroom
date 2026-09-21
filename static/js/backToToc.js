document.addEventListener('DOMContentLoaded', function() {
  const tocLink = document.querySelector('#toc-main');

  if (!tocLink) {
    console.debug("This page does not have a TOC!")
    return;
  }

  document.querySelectorAll('div.post-content > h1, h2, h3').forEach(header => {
    const anchor = document.createElement('a');
    anchor.href = '#toc-main'
    anchor.className = 'toc-anchor';
    anchor.setAttribute('aria-label', 'Back to Table of Contents');

    header.replaceWith(anchor);
    anchor.appendChild(header);
  });
});
