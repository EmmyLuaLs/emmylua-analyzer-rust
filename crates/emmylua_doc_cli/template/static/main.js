// Interactive behavior: search (lazy index, keyboard navigation, match
// highlighting), live sidebar filtering, mobile sidebar drawer, theme toggle,
// copy buttons, and scroll-position persistence across page navigations.
(function () {
  'use strict';

  var SIDEBAR_SCROLL_KEY = 'luafmt-sidebar-scroll';
  var THEME_KEY = 'emmylua-doc-theme';

  function onReady(fn) {
    if (document.readyState !== 'loading') {
      fn();
    } else {
      document.addEventListener('DOMContentLoaded', fn);
    }
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return {
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;',
      }[c];
    });
  }

  function rootPrefix() {
    return window.LUAFMT_ROOT || '';
  }

  // ─── Theme toggle ──────────────────────────────────────
  function initTheme() {
    var btn = document.getElementById('theme-toggle');
    if (!btn) {
      return;
    }
    function isDark() {
      var t = document.documentElement.getAttribute('data-theme');
      if (t) {
        return t === 'dark';
      }
      return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    }
    function syncButton() {
      btn.classList.toggle('is-dark', isDark());
      btn.setAttribute('aria-pressed', isDark() ? 'true' : 'false');
    }
    syncButton();
    if (window.matchMedia) {
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', syncButton);
    }
    btn.addEventListener('click', function () {
      var next = isDark() ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', next);
      try {
        localStorage.setItem(THEME_KEY, next);
      } catch (e) {
        /* private mode etc. */
      }
      syncButton();
    });
  }

  // ─── Mobile sidebar drawer ─────────────────────────────
  function initDrawer() {
    var btn = document.getElementById('sidebar-toggle');
    var sidebar = document.getElementById('sidebar');
    var backdrop = document.getElementById('sidebar-backdrop');
    if (!btn || !sidebar || !backdrop) {
      return;
    }
    function setOpen(open) {
      document.body.classList.toggle('drawer-open', open);
      btn.setAttribute('aria-expanded', open ? 'true' : 'false');
      backdrop.hidden = !open;
    }
    btn.addEventListener('click', function () {
      setOpen(!document.body.classList.contains('drawer-open'));
    });
    backdrop.addEventListener('click', function () {
      setOpen(false);
    });
    document.addEventListener('keydown', function (event) {
      if (event.key === 'Escape' && document.body.classList.contains('drawer-open')) {
        setOpen(false);
        btn.focus();
      }
    });
    // Close the drawer after navigating (mobile only).
    sidebar.addEventListener('click', function (event) {
      if (event.target.closest('a') && window.matchMedia('(max-width: 900px)').matches) {
        setOpen(false);
      }
    });
  }

  // ─── Sidebar scroll persistence ────────────────────────
  function initSidebarScroll() {
    var sidebar = document.getElementById('sidebar');
    if (!sidebar) {
      return;
    }
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

  // ─── Copy buttons on signatures / code blocks ──────────
  function initCopyButtons() {
    var blocks = document.querySelectorAll('.display pre.signature, pre.doc-code');
    blocks.forEach(function (pre) {
      var wrap = document.createElement('div');
      wrap.className = 'code-wrap';
      pre.parentNode.insertBefore(wrap, pre);
      wrap.appendChild(pre);

      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'copy-btn';
      btn.setAttribute('aria-label', 'Copy code');
      btn.innerHTML =
        '<svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">' +
        '<path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25Z"/>' +
        '<path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z"/>' +
        '</svg>';
      wrap.appendChild(btn);

      btn.addEventListener('click', function () {
        var text = pre.textContent || '';
        function done() {
          btn.classList.add('copied');
          setTimeout(function () {
            btn.classList.remove('copied');
          }, 1500);
        }
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () {
            legacyCopy(text);
            done();
          });
        } else {
          legacyCopy(text);
          done();
        }
      });
    });

    function legacyCopy(text) {
      var ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand('copy');
      } catch (e) {
        /* best effort */
      }
      document.body.removeChild(ta);
    }
  }

  // ─── Search ────────────────────────────────────────────
  function initSearch() {
    var input = document.getElementById('search-input');
    var resultsPanel = document.getElementById('search-results');
    var sidebar = document.getElementById('sidebar');
    if (!input || !resultsPanel) {
      return;
    }

    var selected = -1;
    var matches = [];
    var indexRequested = false;

    // The index is fetched lazily on first interaction so every page does not
    // pay the download cost up front.
    function ensureIndex(cb) {
      if (window.SEARCH_INDEX) {
        cb(window.SEARCH_INDEX);
        return;
      }
      if (indexRequested) {
        // A load is in flight; poll briefly for completion.
        var timer = setInterval(function () {
          if (window.SEARCH_INDEX) {
            clearInterval(timer);
            cb(window.SEARCH_INDEX);
          }
        }, 50);
        return;
      }
      indexRequested = true;
      var script = document.createElement('script');
      script.src = rootPrefix() + 'static/search-index.js';
      script.onload = function () {
        cb(window.SEARCH_INDEX || []);
      };
      document.head.appendChild(script);
    }

    function renderResults() {
      if (selected >= matches.length) {
        selected = matches.length - 1;
      }
      resultsPanel.innerHTML = matches
        .map(function (entry, i) {
          var href = rootPrefix() + entry.href;
          return (
            '<a class="search-result' +
            (i === selected ? ' selected' : '') +
            '" role="option" aria-selected="' +
            (i === selected ? 'true' : 'false') +
            '" href="' +
            escapeHtml(href) +
            '"><span class="search-result-kind">' +
            escapeHtml(entry.kind || '') +
            '</span><span class="search-result-name">' +
            entry.nameHtml +
            '</span></a>'
          );
        })
        .join('');
      input.setAttribute('aria-expanded', 'true');
      var sel = resultsPanel.querySelector('.search-result.selected');
      if (sel && sel.scrollIntoView) {
        sel.scrollIntoView({ block: 'nearest' });
      }
    }

    // Exact > prefix > substring, then alphabetical for stability.
    function rank(entry, query) {
      var name = entry.name.toLowerCase();
      if (name === query) {
        return 0;
      }
      if (name.indexOf(query) === 0) {
        return 1;
      }
      return 2;
    }

    function highlight(name, query) {
      var idx = name.toLowerCase().indexOf(query);
      if (idx === -1) {
        return escapeHtml(name);
      }
      return (
        escapeHtml(name.slice(0, idx)) +
        '<mark>' +
        escapeHtml(name.slice(idx, idx + query.length)) +
        '</mark>' +
        escapeHtml(name.slice(idx + query.length))
      );
    }

    function updateSearch() {
      var query = input.value.trim().toLowerCase();
      if (!query) {
        matches = [];
        selected = -1;
        resultsPanel.hidden = true;
        input.setAttribute('aria-expanded', 'false');
        return;
      }
      ensureIndex(function (index) {
        matches = index
          .filter(function (entry) {
            return entry.name.toLowerCase().indexOf(query) !== -1;
          })
          .sort(function (a, b) {
            var d = rank(a, query) - rank(b, query);
            return d !== 0 ? d : a.name.localeCompare(b.name);
          })
          .slice(0, 20)
          .map(function (entry) {
            return {
              href: entry.href,
              kind: entry.kind,
              name: entry.name,
              nameHtml: highlight(entry.name, query),
            };
          });
        selected = -1;
        if (matches.length === 0) {
          resultsPanel.innerHTML =
            '<div class="search-result-empty">No results for &ldquo;' + escapeHtml(input.value) + '&rdquo;</div>';
          resultsPanel.hidden = false;
          return;
        }
        renderResults();
        resultsPanel.hidden = false;
      });
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
        var anyVisible = Array.prototype.some.call(details.querySelectorAll('a[data-name]'), function (a) {
          return a.style.display !== 'none';
        });
        if (query !== '' && anyVisible && !details.open) {
          details.open = true;
        }
        details.style.display = anyVisible ? '' : 'none';
      });
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
        var entry = selected >= 0 ? matches[selected] : matches[0];
        if (entry) {
          window.location.href = rootPrefix() + entry.href;
        }
      } else if (event.key === 'Escape') {
        resultsPanel.hidden = true;
        input.setAttribute('aria-expanded', 'false');
        input.blur();
      }
    });

    input.addEventListener('blur', function () {
      // Allow the click on a result to register before hiding.
      setTimeout(function () {
        resultsPanel.hidden = true;
        input.setAttribute('aria-expanded', 'false');
      }, 120);
    });

    // "/" or Ctrl+K focuses the search box, like rustdoc / docs sites.
    document.addEventListener('keydown', function (event) {
      var tag = (event.target.tagName || '').toLowerCase();
      var typing = tag === 'input' || tag === 'textarea' || event.target.isContentEditable;
      if (typing) {
        return;
      }
      if (event.key === '/' || ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k')) {
        event.preventDefault();
        input.focus();
        input.select();
      }
    });
  }

  // ─── Member accordion helpers ──────────────────────────
  // Members are <details> elements. A deep link (#name) should open the
  // targeted member, and TOC links should open their target as well.
  function initMembers() {
    function openTarget() {
      var id = decodeURIComponent(window.location.hash.slice(1));
      if (!id) {
        return;
      }
      var el = document.getElementById(id);
      if (el && el.tagName === 'DETAILS' && !el.open) {
        el.open = true;
      }
    }
    openTarget();
    window.addEventListener('hashchange', openTarget);

    // Clicking a TOC link pointing at a collapsed member opens it.
    document.querySelectorAll('.toc-list a[href^="#"]').forEach(function (link) {
      link.addEventListener('click', function () {
        var el = document.getElementById(link.getAttribute('href').slice(1));
        if (el && el.tagName === 'DETAILS') {
          el.open = true;
        }
      });
    });
  }

  // ─── TOC scrollspy ─────────────────────────────────────
  function initScrollSpy() {
    var toc = document.getElementById('toc');
    if (!toc) {
      return;
    }
    var links = Array.prototype.slice.call(toc.querySelectorAll('a[href^="#"]'));
    if (links.length === 0) {
      return;
    }
    var targets = links
      .map(function (link) {
        var el = document.getElementById(link.getAttribute('href').slice(1));
        return el ? { link: link, el: el } : null;
      })
      .filter(Boolean);
    if (targets.length === 0) {
      return;
    }
    var current = null;
    function update() {
      // The active entry is the last target whose top is above the marker line.
      var line = 140;
      var best = null;
      for (var i = 0; i < targets.length; i++) {
        var top = targets[i].el.getBoundingClientRect().top;
        if (top <= line) {
          best = targets[i];
        } else {
          break;
        }
      }
      if (!best) {
        best = targets[0];
      }
      if (best === current) {
        return;
      }
      current = best;
      links.forEach(function (l) {
        l.classList.remove('active');
      });
      best.link.classList.add('active');
    }
    var ticking = false;
    document.addEventListener(
      'scroll',
      function () {
        if (!ticking) {
          ticking = true;
          requestAnimationFrame(function () {
            update();
            ticking = false;
          });
        }
      },
      { passive: true },
    );
    update();
  }

  onReady(function () {
    initTheme();
    initDrawer();
    initSidebarScroll();
    initCopyButtons();
    initSearch();
    initMembers();
    initScrollSpy();
  });
})();
