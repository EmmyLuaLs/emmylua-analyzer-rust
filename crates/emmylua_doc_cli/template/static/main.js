// Sidebar behavior: search results panel with keyboard navigation, live sidebar
// filtering, and scroll-position persistence across page navigations.
(function () {
  'use strict';

  var SIDEBAR_SCROLL_KEY = 'luafmt-sidebar-scroll';

  function onReady(fn) {
    if (document.readyState !== 'loading') {
      fn();
    } else {
      document.addEventListener('DOMContentLoaded', fn);
    }
  }

  onReady(function () {
    var input = document.getElementById('search-input');
    var resultsPanel = document.getElementById('search-results');
    var sidebar = document.getElementById('sidebar');
    var searchIndex = window.SEARCH_INDEX || [];
    var selected = -1;
    var matches = [];

    function rootPrefix() {
      return window.LUAFMT_ROOT || '';
    }

    function renderResults() {
      if (selected >= matches.length) {
        selected = matches.length - 1;
      }
      resultsPanel.innerHTML = matches
        .map(function (entry, i) {
          var href = rootPrefix() + entry.href;
          return '<a class="search-result' + (i === selected ? ' selected' : '') +
            '" href="' + href + '"><span class="search-result-kind">' +
            (entry.kind || '') + '</span><span class="search-result-name">' +
            entry.name + '</span></a>';
        })
        .join('');
      var sel = resultsPanel.querySelector('.search-result.selected');
      if (sel && sel.scrollIntoView) {
        sel.scrollIntoView({ block: 'nearest' });
      }
    }

    function updateSearch() {
      var query = input.value.trim().toLowerCase();
      if (!query) {
        matches = [];
        selected = -1;
        resultsPanel.hidden = true;
        return;
      }
      matches = searchIndex
        .filter(function (entry) {
          return entry.name.toLowerCase().indexOf(query) !== -1;
        })
        .slice(0, 20);
      selected = -1;
      if (matches.length === 0) {
        resultsPanel.innerHTML =
          '<div class="search-result-empty">No results for &ldquo;' +
          input.value.replace(/[&<>]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]; }) +
          '&rdquo;</div>';
        resultsPanel.hidden = false;
        return;
      }
      renderResults();
      resultsPanel.hidden = false;
    }

    function filterSidebar() {
      if (!sidebar) {
        return;
      }
      var query = input.value.trim().toLowerCase();
      sidebar.querySelectorAll('a[data-name]').forEach(function (link) {
        var name = (link.getAttribute('data-name') || '').toLowerCase();
        link.style.display = query === '' || name.indexOf(query) !== -1 ? '' : 'none';
      });
      sidebar.querySelectorAll('details').forEach(function (details) {
        var anyVisible = Array.prototype.some.call(
          details.querySelectorAll('a[data-name]'),
          function (a) { return a.style.display !== 'none'; }
        );
        if (query !== '' && anyVisible && !details.open) {
          details.open = true;
        }
        details.style.display = anyVisible ? '' : 'none';
      });
    }

    if (sidebar) {
      var saved = parseInt(sessionStorage.getItem(SIDEBAR_SCROLL_KEY), 10);
      if (!isNaN(saved)) {
        sidebar.scrollTop = saved;
      }
      sidebar.addEventListener('scroll', function () {
        sessionStorage.setItem(SIDEBAR_SCROLL_KEY, String(sidebar.scrollTop));
      });

      // Bring the active nav item into view (e.g. after opening its branch).
      var activeLink = sidebar.querySelector('a.nav-item.active');
      if (activeLink && activeLink.scrollIntoView) {
        activeLink.scrollIntoView({ block: 'center' });
      }
    }

    if (!input || !resultsPanel) {
      return;
    }

    input.addEventListener('input', function () {
      updateSearch();
      filterSidebar();
    });

    input.addEventListener('keydown', function (event) {
      if (resultsPanel.hidden) {
        return;
      }
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        selected = Math.min(selected + 1, matches.length - 1);
        renderResults();
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        selected = Math.max(selected - 1, 0);
        renderResults();
      } else if (event.key === 'Enter') {
        var entry = matches[selected];
        if (entry) {
          window.location.href = rootPrefix() + entry.href;
        }
      } else if (event.key === 'Escape') {
        resultsPanel.hidden = true;
        input.blur();
      }
    });

    input.addEventListener('blur', function () {
      // Allow the click on a result to register before hiding.
      setTimeout(function () { resultsPanel.hidden = true; }, 120);
    });
  });
})();
